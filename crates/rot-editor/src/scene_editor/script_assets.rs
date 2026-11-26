use rot_assets::AssetStore;
use rot_scene::{component_scene::ComponentScene, BehaviorSource};
use std::collections::BTreeMap;

const _: () = assert!(rot_assets::MAX_LUA_SOURCE_BYTES == rot_scene::MAX_SOURCE_BYTES);

pub(super) fn source(path: &str, text: String) -> BehaviorSource {
    const MAX_LABEL_BYTES: usize = 128;
    let clean: String = path
        .chars()
        .map(|ch| if ch.is_control() { '�' } else { ch })
        .collect();
    let clean = clean.trim();
    let label = if clean.len() <= MAX_LABEL_BYTES {
        clean.to_owned()
    } else {
        let prefix = "…/";
        let cutoff = clean.ceil_char_boundary(clean.len() - (MAX_LABEL_BYTES - prefix.len()));
        format!("{prefix}{}", &clean[cutoff..])
    };
    BehaviorSource {
        label: Some(label),
        language_id: rot_behavior::LANGUAGE_ID.into(),
        host_api_version: rot_behavior::INPUT_HOST_API_VERSION,
        text,
    }
}

pub(crate) fn reload_candidate(
    store: &AssetStore,
    asset_id: &str,
    observed_hash: &str,
) -> Option<(ProjectSource, BehaviorSource)> {
    let (info, text) = store.ready_script(asset_id)?;
    let hash = info.hash.as_ref()?;
    (hash != observed_hash).then(|| {
        (
            ProjectSource {
                asset_id: asset_id.to_owned(),
                source: info.source.clone(),
                accepted_hash: hash.clone(),
            },
            source(&info.source, text.to_string()),
        )
    })
}

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct ProjectSource {
    pub asset_id: String,
    pub source: String,
    pub accepted_hash: String,
}

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceStatus {
    Ready,
    Changed,
    Unavailable,
}

#[derive(serde::Serialize)]
pub(crate) struct SourceObservation<'a> {
    #[serde(flatten)]
    pub captured: &'a ProjectSource,
    pub current_source: Option<&'a str>,
    pub status: SourceStatus,
}

impl ProjectSource {
    pub(crate) fn observe<'a>(&'a self, assets: Option<&'a AssetStore>) -> SourceObservation<'a> {
        let info = assets
            .and_then(|store| store.asset_info(&self.asset_id))
            .filter(|info| info.kind == rot_assets::AssetKind::Script);
        let status = match info {
            Some(info) if info.status == rot_assets::AssetStatus::Ready => {
                if info.hash.as_deref() == Some(&self.accepted_hash) {
                    SourceStatus::Ready
                } else {
                    SourceStatus::Changed
                }
            }
            _ => SourceStatus::Unavailable,
        };
        SourceObservation {
            captured: self,
            current_source: info.map(|info| info.source.as_str()),
            status,
        }
    }
}

pub(crate) type ProjectSources = BTreeMap<String, ProjectSource>;

pub(crate) fn resolved(
    scene: &ComponentScene,
    assets: Option<&AssetStore>,
    verify: bool,
) -> Result<Option<ComponentScene>, String> {
    resolve(scene, assets, verify, None)
}

pub(crate) fn resolved_for_play(
    scene: &ComponentScene,
    assets: Option<&AssetStore>,
) -> Result<(Option<ComponentScene>, ProjectSources), String> {
    let mut provenance = BTreeMap::new();
    let scene = resolve(scene, assets, true, Some(&mut provenance))?;
    Ok((scene, provenance))
}

fn resolve(
    scene: &ComponentScene,
    assets: Option<&AssetStore>,
    verify: bool,
    provenance: Option<&mut ProjectSources>,
) -> Result<Option<ComponentScene>, String> {
    let ids = scene.script_asset_ids();
    if ids.is_empty() {
        return Ok(None);
    }
    let store = assets.ok_or_else(|| "Lua sources need a ready Assets catalog.".to_owned())?;
    if verify {
        for object in &scene.objects {
            for component in &object.components {
                let properties = match &component.kind {
                    rot_scene::component_scene::Component::Script {
                        binding,
                        source_kind: Some(_),
                        ..
                    } if component.enabled => &binding.properties,
                    _ => continue,
                };
                for (name, value) in properties {
                    if let rot_scene::PropertyValue::SoundRef(cue) = value {
                        let asset = cue.asset_id.as_deref().unwrap_or("");
                        if store
                            .asset_info(asset)
                            .is_none_or(|info| info.status != rot_assets::AssetStatus::Ready)
                            || store.sound(asset).is_none()
                        {
                            return Err(format!(
                                "Sound cue {name:?} on {} needs a ready WAV ({asset}).",
                                object.name
                            ));
                        }
                    }
                }
            }
        }
    }
    let mut sources = BTreeMap::new();
    for id in ids {
        let (info, text) = if verify {
            store
                .verify_script(id)
                .map_err(|failure| failure.to_string())?
        } else {
            store.ready_script(id).ok_or_else(|| {
                format!(
                    "Lua source {} is unavailable; repair the file in Assets.",
                    store.asset_info(id).map_or(id, |info| info.source.as_str())
                )
            })?
        };
        sources.insert(id.to_owned(), source(&info.source, text.to_string()));
    }
    let mut resolved = scene.clone();
    let keys = resolved
        .bake_script_assets_with_keys(sources)
        .map_err(|failure| failure.to_string())?;
    if let Some(provenance) = provenance {
        for (asset_id, variants) in keys {
            let info = store.asset_info(&asset_id).expect("resolved Lua asset");
            let accepted_hash = info.hash.as_ref().expect("accepted Lua hash");
            for key in variants.into_values() {
                provenance.insert(
                    key,
                    ProjectSource {
                        asset_id: asset_id.clone(),
                        source: info.source.clone(),
                        accepted_hash: accepted_hash.clone(),
                    },
                );
            }
        }
    }
    Ok(Some(resolved))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::component_scene::{Component, ComponentInstance, ScriptSourceKind};
    use rot_scene::{BehaviorBinding, BindingTarget, Scene, Session};

    #[test]
    fn file_script_cues_are_independent_and_bake_into_the_sound_closure() {
        use rot_scene::input::{ActionFrame, ActionValue, ButtonState};
        use rot_scene::{PropertyValue, SoundRef};

        let dir = tempfile::tempdir().unwrap();
        let assets_dir = dir.path().join("Assets");
        std::fs::create_dir(&assets_dir).unwrap();
        std::fs::write(
            assets_dir.join("shared.lua"),
            r#"
            return {update=function(transform, properties, dt, host)
                if host.button("fire").pressed then host.play_sound("confirm") end
                return transform
            end}
        "#,
        )
        .unwrap();
        let wav = b"RIFF\x26\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x40\x1f\0\0\x80\x3e\0\0\x02\0\x10\0data\x02\0\0\0\0\0";
        let mut store = AssetStore::open(dir.path()).unwrap();
        store.import("shared", "Assets/shared.lua").unwrap();
        for id in ["near", "far"] {
            std::fs::write(assets_dir.join(format!("{id}.wav")), wav).unwrap();
            store.import(id, &format!("Assets/{id}.wav")).unwrap();
        }

        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.actions = serde_json::from_value(serde_json::json!({
            "fire":{"kind":{"type":"button","keys":["space"]}}
        }))
        .unwrap();
        for (object, (asset, gain)) in scene
            .objects
            .iter_mut()
            .zip([("near", 0.25), ("far", 0.75)])
        {
            object.components.push(ComponentInstance {
                id: format!("cue-{asset}"),
                enabled: true,
                kind: Component::Script {
                    source_kind: Some(ScriptSourceKind::Asset),
                    movement: Default::default(),
                    binding: BehaviorBinding {
                        target: BindingTarget::Object(object.id.clone()),
                        source_id: "shared".into(),
                        enabled: true,
                        properties: BTreeMap::from([(
                            "confirm".into(),
                            PropertyValue::SoundRef(SoundRef {
                                asset_id: Some(asset.into()),
                                gain,
                                playback: Default::default(),
                            }),
                        )]),
                        state_defaults: Default::default(),
                        commands: Default::default(),
                    },
                },
            });
        }
        scene.validate().unwrap();
        let (baked, provenance) = resolved_for_play(&scene, Some(&store)).unwrap();
        let baked = baked.unwrap();
        assert_eq!(baked.sources.len(), 1);
        let track = &provenance[baked.sources.keys().next().unwrap()];
        assert_eq!(track.asset_id, "shared");
        assert_eq!(track.source, "Assets/shared.lua");
        assert_eq!(
            track.accepted_hash,
            rot_behavior::source_hash(store.script("shared").unwrap())
        );
        assert!(track.observe(Some(&store)).status == SourceStatus::Ready);
        assert_eq!(
            baked.sound_asset_references().assets,
            ["far".into(), "near".into()].into()
        );
        let mut engine = rot_behavior::Engine::new(Default::default()).unwrap();
        let mut runtime = engine.reset_component(&baked, 0).unwrap();
        assert!(runtime.audio_batch().is_none());
        engine
            .step_with_input(
                &mut runtime,
                &ActionFrame(BTreeMap::from([(
                    "fire".into(),
                    ActionValue::Button(ButtonState {
                        held: true,
                        pressed: true,
                        released: false,
                    }),
                )])),
            )
            .unwrap();
        let cues = &runtime.audio_batch().unwrap().cues;
        assert_eq!(cues.len(), 2);
        assert!(cues
            .iter()
            .any(|cue| cue.binding_id == "cue-near" && cue.asset_id == "near" && cue.gain == 0.25));
        assert!(cues
            .iter()
            .any(|cue| cue.binding_id == "cue-far" && cue.asset_id == "far" && cue.gain == 0.75));
    }

    #[test]
    fn verified_file_uses_button_edges_and_signed_axis_in_play() {
        use rot_scene::component_scene::{Geometry, ObjectNode};
        use rot_scene::input::{ActionFrame, ActionValue, ButtonState};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Assets/move.lua");
        std::fs::create_dir(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"
            return {
                update = function(object, properties, dt, host)
                    object.translation[1] = object.translation[1] + 6 * host.axis("move") * dt
                    if host.button("fire").pressed then
                        object.rotation[2] = object.rotation[2] + 0.5235987756
                    end
                    return object
                end
            }
        "#,
        )
        .unwrap();
        let mut assets = AssetStore::open(dir.path()).unwrap();
        assets.import("mover", "Assets/move.lua").unwrap();
        let mut scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("keys")).unwrap()).unwrap();
        scene.actions = serde_json::from_value(serde_json::json!({
            "move":{"kind":{"type":"axis1d","negative":["a"],"positive":["d"]}},
            "fire":{"kind":{"type":"button","keys":["space"]}}
        }))
        .unwrap();
        scene.objects.push(ObjectNode {
            id: "cube".into(),
            name: "Cube".into(),
            parent: None,
            enabled: true,
            transform: Default::default(),
            components: vec![
                ComponentInstance {
                    id: "mesh".into(),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Cube,
                        tint: [1.; 4],
                        material: None,
                    },
                },
                ComponentInstance {
                    id: "script".into(),
                    enabled: true,
                    kind: Component::Script {
                        binding: BehaviorBinding {
                            target: BindingTarget::Object("cube".into()),
                            source_id: "mover".into(),
                            enabled: true,
                            properties: BTreeMap::new(),
                            commands: BTreeMap::new(),
                            state_defaults: BTreeMap::new(),
                        },
                        source_kind: Some(ScriptSourceKind::Asset),
                        movement: Default::default(),
                    },
                },
            ],
        });
        let resolved = resolved(&scene, Some(&assets), true).unwrap().unwrap();
        let mut engine = rot_behavior::Engine::new(rot_behavior::Limits::default()).unwrap();
        let mut runtime = engine.reset_component(&resolved, 0).unwrap();
        for (value, pressed) in [(1, true), (-1, false), (1, false)] {
            let frame = ActionFrame(
                [
                    ("move".into(), ActionValue::Axis1d { value }),
                    (
                        "fire".into(),
                        ActionValue::Button(ButtonState {
                            held: true,
                            pressed,
                            released: false,
                        }),
                    ),
                ]
                .into(),
            );
            engine.step_with_input(&mut runtime, &frame).unwrap();
        }
        let pose = &runtime.component_transforms().unwrap()[0];
        assert!((pose.translation[0] - 0.1).abs() < 0.000001);
        assert!((pose.rotation[1] - std::f32::consts::FRAC_PI_6).abs() < 0.000001);
        assert!(scene.sources.is_empty());
    }

    #[test]
    fn separately_baked_movement_modes_share_stable_diagnostic_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join("Assets")).unwrap();
        std::fs::write(root.join("Assets/drive.lua"), "return {}").unwrap();
        let mut assets = AssetStore::open(root).unwrap();
        assets.import("drive", "Assets/drive.lua").unwrap();
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        for (index, object) in scene.objects.iter_mut().take(2).enumerate() {
            object.components.push(ComponentInstance {
                id: format!("drive-{index}"),
                enabled: true,
                kind: Component::Script {
                    source_kind: Some(ScriptSourceKind::Asset),
                    movement: if index == 0 {
                        Default::default()
                    } else {
                        rot_scene::component_scene::ScriptMovement::KinematicSphere
                    },
                    binding: BehaviorBinding {
                        target: BindingTarget::Object(object.id.clone()),
                        source_id: "drive".into(),
                        enabled: true,
                        properties: Default::default(),
                        commands: Default::default(),
                        state_defaults: Default::default(),
                    },
                },
            });
        }
        let (baked, provenance) = resolved_for_play(&scene, Some(&assets)).unwrap();
        let baked = baked.unwrap();
        assert_eq!(provenance.len(), 2);
        for (key, source) in baked.sources {
            let original = &provenance[&key];
            assert_eq!(original.asset_id, "drive");
            assert_eq!(
                original.accepted_hash,
                rot_behavior::source_hash(&source.text)
            );
        }

        let diagnostic_source = provenance.values().next().unwrap();
        assets.rename("drive", "Assets/drive renamed.lua").unwrap();
        let renamed = diagnostic_source.observe(Some(&assets));
        assert_eq!(renamed.current_source, Some("Assets/drive renamed.lua"));
        assert!(renamed.status == SourceStatus::Ready);
        std::fs::write(
            root.join("Assets/drive renamed.lua"),
            "return { other = true }",
        )
        .unwrap();
        assets.refresh("drive").unwrap();
        assert!(diagnostic_source.observe(Some(&assets)).status == SourceStatus::Changed);
        assert!(diagnostic_source.observe(None).status == SourceStatus::Unavailable);
    }

    #[test]
    fn resolved_script_never_heals_an_unrelated_dangling_embedded_binding() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Assets/spin.lua");
        std::fs::create_dir(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "return {}").unwrap();
        let mut assets = AssetStore::open(dir.path()).unwrap();
        assets.import("spin", "Assets/spin.lua").unwrap();
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        let object = &mut scene.objects[0];
        for (id, source_id, source_kind) in [
            ("embedded", "asset-lua-1", None),
            ("file", "spin", Some(ScriptSourceKind::Asset)),
        ] {
            object.components.push(ComponentInstance {
                id: id.into(),
                enabled: true,
                kind: Component::Script {
                    binding: BehaviorBinding {
                        target: BindingTarget::Object(object.id.clone()),
                        source_id: source_id.into(),
                        enabled: true,
                        properties: BTreeMap::new(),
                        commands: BTreeMap::new(),
                        state_defaults: BTreeMap::new(),
                    },
                    source_kind,
                    movement: Default::default(),
                },
            });
        }
        let original = scene.clone();
        let baked = resolved(&scene, Some(&assets), true).unwrap().unwrap();
        assert!(!baked.sources.contains_key("asset-lua-1"));
        assert!(baked.sources.contains_key("asset-lua-2"));
        assert!(matches!(
            &baked.objects[0].components[1].kind,
            Component::Script { binding, source_kind: None, .. }
                if binding.source_id == "asset-lua-1"));
        assert_eq!(scene, original);
    }
}
