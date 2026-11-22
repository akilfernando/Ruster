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
        host_api_version: rot_behavior::HOST_API_VERSION,
        text,
    }
}

pub(crate) fn resolved(
    scene: &ComponentScene,
    assets: Option<&AssetStore>,
    verify: bool,
) -> Result<Option<ComponentScene>, String> {
    let ids = scene.script_asset_ids();
    if ids.is_empty() {
        return Ok(None);
    }
    let store = assets.ok_or_else(|| "Lua sources need a ready Assets catalog.".to_owned())?;
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
    resolved
        .bake_script_assets(sources)
        .map_err(|failure| failure.to_string())?;
    Ok(Some(resolved))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::component_scene::{Component, ComponentInstance, ScriptSourceKind};
    use rot_scene::{BehaviorBinding, BindingTarget, Scene, Session};

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
                },
            });
        }
        let original = scene.clone();
        let baked = resolved(&scene, Some(&assets), true).unwrap().unwrap();
        assert!(!baked.sources.contains_key("asset-lua-1"));
        assert!(baked.sources.contains_key("asset-lua-2"));
        assert!(matches!(
            &baked.objects[0].components[1].kind,
            Component::Script { binding, source_kind: None }
                if binding.source_id == "asset-lua-1"));
        assert_eq!(scene, original);
    }
}
