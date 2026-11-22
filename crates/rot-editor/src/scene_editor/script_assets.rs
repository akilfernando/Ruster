use rot_assets::AssetStore;
use rot_scene::{component_scene::ComponentScene, BehaviorSource};
use std::collections::BTreeMap;

const _: () = assert!(rot_assets::MAX_LUA_SOURCE_BYTES == rot_scene::MAX_SOURCE_BYTES);

pub(super) fn source(path: &str, text: String) -> BehaviorSource {
    let label = path.trim();
    BehaviorSource {
        label: (!label.is_empty() && label.len() <= 128 && !label.chars().any(char::is_control))
            .then(|| label.to_owned()),
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
