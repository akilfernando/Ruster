//! Shared material wire parsing and validation helpers.
use rot_scene::OpaqueMaterial;
use serde_json::{Map, Value};

type CommandError = (String, String);

fn error(code: &str, message: impl Into<String>) -> CommandError {
    (code.into(), message.into())
}

pub(super) fn exact_keys(map: &Map<String, Value>, required: &[&str], optional: &[&str]) -> bool {
    required.iter().all(|field| map.contains_key(*field))
        && map
            .keys()
            .all(|field| required.contains(&field.as_str()) || optional.contains(&field.as_str()))
}

pub(super) fn number(value: &Value) -> Result<u64, CommandError> {
    let text = value
        .as_str()
        .filter(|text| {
            !text.is_empty()
                && (*text == "0" || !text.starts_with('0'))
                && text.bytes().all(|byte| byte.is_ascii_digit())
        })
        .ok_or_else(|| {
            error(
                "invalid_arguments",
                "Expected a canonical decimal u64 string",
            )
        })?;
    text.parse()
        .map_err(|_| error("invalid_arguments", "Decimal value exceeds u64"))
}

pub(super) fn id(value: &Value) -> Result<&str, CommandError> {
    let id = value
        .as_str()
        .ok_or_else(|| error("invalid_arguments", "Expected a stable ID string"))?;
    rot_scene::validate_id(id)
        .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
    Ok(id)
}

pub(super) fn optional_name(value: Option<&Value>) -> Result<Option<String>, CommandError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(name)) => {
            let candidate = OpaqueMaterial {
                name: Some(name.clone()),
                ..OpaqueMaterial::default()
            };
            candidate
                .validate()
                .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
            Ok(candidate.name)
        }
        _ => Err(error(
            "invalid_arguments",
            "Display name must be string or null",
        )),
    }
}

fn rgb(value: &Value, label: &str) -> Result<[f32; 3], CommandError> {
    let tint = value
        .as_array()
        .filter(|tint| tint.len() == 3)
        .ok_or_else(|| {
            error(
                "invalid_arguments",
                format!("{label} requires three numeric channels"),
            )
        })?;
    let mut channels = [0.0; 3];
    for (channel, value) in channels.iter_mut().zip(tint) {
        // Validate the wire number before f32 narrowing can round into range.
        *channel = value
            .as_f64()
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            .ok_or_else(|| {
                error(
                    "invalid_arguments",
                    format!("{label} must be finite in 0..=1"),
                )
            })? as f32;
    }
    Ok(channels)
}

pub(super) fn material(
    value: &Value,
    prior: Option<&OpaqueMaterial>,
) -> Result<OpaqueMaterial, CommandError> {
    let map = value
        .as_object()
        .filter(|map| {
            exact_keys(
                map,
                &["tint", "base_color_texture", "filter", "address"],
                &[
                    "name",
                    "response",
                    "roughness_texture",
                    "metallic_texture",
                    "normal_texture",
                    "emissive_texture",
                    "occlusion_texture",
                    "emission_color",
                    "metallic",
                    "normal_scale",
                    "occlusion_strength",
                    "double_sided",
                    "opacity",
                    "alpha_mode",
                    "alpha_cutoff",
                    "uv_tiling",
                    "uv_offset",
                ],
            )
        })
        .ok_or_else(|| {
            error(
                "invalid_arguments",
                "Expected the complete opaque material shape",
            )
        })?;
    let scalar = |key: &str, min: f64, max: f64, fallback: f32, label: &str| match map.get(key) {
        None => Ok(fallback),
        Some(value) => value
            .as_f64()
            .filter(|v| v.is_finite() && (min..=max).contains(v))
            .map(|v| v as f32)
            .ok_or_else(|| {
                error(
                    "invalid_arguments",
                    format!("{label} must be finite in {min}..={max}"),
                )
            }),
    };
    let optional_slot = |key: &str| -> Result<Option<String>, CommandError> {
        match map.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => Ok(Some(id(value)?.into())),
        }
    };
    let uv_pair = |key: &str, fallback: [f32; 2]| {
        map.get(key)
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()
            .map(|value| value.unwrap_or(fallback))
            .map_err(|_| {
                error(
                    "invalid_arguments",
                    format!("{key} must be two finite numbers"),
                )
            })
    };
    let candidate = OpaqueMaterial {
        uv_tiling: uv_pair("uv_tiling", prior.map_or([1.0; 2], |value| value.uv_tiling))?,
        uv_offset: uv_pair("uv_offset", prior.map_or([0.0; 2], |value| value.uv_offset))?,
        name: optional_name(map.get("name"))?,
        response: map
            .get("response")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()
            .map_err(|e| {
                error(
                    "invalid_arguments",
                    format!("Invalid material response: {e}"),
                )
            })?
            .unwrap_or_default(),
        tint: rgb(&map["tint"], "Tint")?,
        emission_color: match map.get("emission_color") {
            Some(value) => rgb(value, "Emission Color")?,
            None => prior.map_or([0.0; 3], |prior| prior.emission_color),
        },
        metallic: scalar(
            "metallic",
            0.0,
            1.0,
            prior.map_or(0.0, |prior| prior.metallic),
            "Metallic",
        )?,
        normal_scale: scalar(
            "normal_scale",
            0.0,
            2.0,
            prior.map_or(1.0, |prior| prior.normal_scale),
            "Normal Scale",
        )?,
        occlusion_strength: scalar(
            "occlusion_strength",
            0.0,
            1.0,
            prior.map_or(1.0, |prior| prior.occlusion_strength),
            "Occlusion Strength",
        )?,
        double_sided: match map.get("double_sided") {
            Some(value) => value
                .as_bool()
                .ok_or_else(|| error("invalid_arguments", "Double Sided must be true or false"))?,
            None => prior.is_some_and(|material| material.double_sided),
        },
        opacity: scalar(
            "opacity",
            0.0,
            1.0,
            prior.map_or(1.0, |prior| prior.opacity),
            "Opacity",
        )?,
        alpha_mode: match map.get("alpha_mode") {
            Some(value) => serde_json::from_value(value.clone()).map_err(|_| {
                error(
                    "invalid_arguments",
                    "Alpha Mode must be opaque, mask, or blend",
                )
            })?,
            None => prior.map_or(rot_scene::AlphaMode::Opaque, |material| material.alpha_mode),
        },
        alpha_cutoff: scalar(
            "alpha_cutoff",
            0.0,
            1.0,
            prior.map_or(0.5, |prior| prior.alpha_cutoff),
            "Alpha Cutoff",
        )?,
        base_color_texture: if map["base_color_texture"].is_null() {
            None
        } else {
            Some(id(&map["base_color_texture"])?.into())
        },
        roughness_texture: optional_slot("roughness_texture")?,
        metallic_texture: optional_slot("metallic_texture")?,
        normal_texture: optional_slot("normal_texture")?,
        emissive_texture: optional_slot("emissive_texture")?,
        occlusion_texture: optional_slot("occlusion_texture")?,
        filter: serde_json::from_value(map["filter"].clone())
            .map_err(|_| error("invalid_arguments", "Filter must be nearest or linear"))?,
        address: serde_json::from_value(map["address"].clone())
            .map_err(|_| error("invalid_arguments", "Address must be repeat or clamp"))?,
    };
    candidate
        .validate()
        .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
    Ok(candidate)
}
