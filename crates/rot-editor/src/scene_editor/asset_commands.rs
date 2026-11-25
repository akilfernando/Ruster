//! Asset argument parsing and mutation builders shared with v23 component asset writes.
use rot_assets::{AssetMutation, AssetStore, MeshNormalPolicy, TextureRole};
use serde_json::Value;

type Error = (String, String);

fn error(code: &str, text: impl Into<String>) -> Error {
    (code.into(), text.into())
}

pub(super) fn asset_arguments<'a>(
    op: &str,
    args: &'a Value,
) -> Result<&'a serde_json::Map<String, Value>, Error> {
    let fields: &[&str] = if op == "asset_refresh" {
        &[
            "expected_session_token",
            "expected_revision",
            "expected_asset_revision",
            "asset_id",
        ]
    } else {
        &[
            "expected_session_token",
            "expected_revision",
            "expected_asset_revision",
            "asset_id",
            "source",
        ]
    };
    let map = args
        .as_object()
        .ok_or_else(|| error("invalid_arguments", "Expected object arguments"))?;
    if !matches!(op, "asset_import" | "asset_refresh")
        || fields.iter().any(|key| !map.contains_key(*key))
        || map.keys().any(|key| {
            !fields.contains(&key.as_str())
                && !(op == "asset_import"
                    && matches!(key.as_str(), "normal_policy" | "texture_role"))
        })
    {
        return Err(error("invalid_arguments", "Unknown or missing argument"));
    }
    Ok(map)
}

pub(super) fn asset_mutation(
    store: &AssetStore,
    op: &str,
    args: &Value,
    map: &serde_json::Map<String, Value>,
) -> Result<AssetMutation, Error> {
    let id = args["asset_id"]
        .as_str()
        .ok_or_else(|| error("invalid_arguments", "asset_id required"))?
        .to_owned();
    Ok(match op {
        "asset_refresh" => {
            if store.model(&id).is_some() {
                AssetMutation::RefreshGlb { id }
            } else {
                AssetMutation::Refresh { id }
            }
        }
        "asset_import" => {
            let source = args["source"]
                .as_str()
                .ok_or_else(|| error("invalid_arguments", "source required"))?;
            if map.contains_key("normal_policy") && map.contains_key("texture_role") {
                return Err(error(
                    "invalid_arguments",
                    "normal_policy and texture_role cannot be combined",
                ));
            }
            if let Some(role) = map.get("texture_role") {
                if !source.ends_with(".png") {
                    return Err(error(
                        "invalid_arguments",
                        "texture_role is available only for .png texture imports",
                    ));
                }
                let role = match role.as_str() {
                    Some("base_color_srgb") => TextureRole::BaseColor,
                    Some("roughness_linear") => TextureRole::Roughness,
                    _ => {
                        return Err(error(
                            "invalid_arguments",
                            "texture_role must be base_color_srgb or roughness_linear",
                        ));
                    }
                };
                AssetMutation::ImportTexture {
                    id,
                    source: source.into(),
                    role,
                }
            } else if let Some(policy) = map.get("normal_policy") {
                if !source.ends_with(".obj") {
                    return Err(error(
                        "invalid_arguments",
                        "normal_policy is available only for .obj mesh imports",
                    ));
                }
                let normal_policy = match policy.as_str() {
                    Some("flat") => MeshNormalPolicy::Flat,
                    Some("authored_when_complete") => MeshNormalPolicy::AuthoredWhenComplete,
                    _ => {
                        return Err(error(
                            "invalid_arguments",
                            "normal_policy must be flat or authored_when_complete",
                        ));
                    }
                };
                AssetMutation::ImportMesh {
                    id,
                    source: source.into(),
                    normal_policy,
                }
            } else if source.ends_with(".glb") || source.ends_with(".gltf") {
                AssetMutation::ImportGlb {
                    id,
                    source: source.into(),
                }
            } else {
                AssetMutation::Import {
                    id,
                    source: source.into(),
                }
            }
        }
        _ => {
            return Err(error(
                "unsupported_operation",
                format!("Unsupported asset operation: {op}"),
            ))
        }
    })
}

pub(super) fn decimal(value: &Value, key: &str) -> Result<u64, Error> {
    let text = value
        .as_str()
        .filter(|s| {
            !s.is_empty()
                && (*s == "0" || !s.starts_with('0'))
                && s.bytes().all(|b| b.is_ascii_digit())
        })
        .ok_or_else(|| {
            error(
                "invalid_arguments",
                format!("{key} must be canonical decimal string"),
            )
        })?;
    text.parse()
        .map_err(|_| error("invalid_arguments", format!("Invalid {key}")))
}
