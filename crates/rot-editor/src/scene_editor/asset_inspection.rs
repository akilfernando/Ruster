//! Bounded borrowed catalog observations for the native bridge.
use crate::strings::Text;
use rot_assets::{AssetStatus, AssetStore};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

const PAGE_SIZE: usize = 16;
const DIAGNOSTIC_CHARS: usize = 256;
const ISSUE_PAGE_SIZE: usize = 4;
const ISSUE_TEXT_BYTES: usize = 256;

pub(super) struct ImportIssue {
    pub status: AssetStatus,
    pub asset_id: Option<String>,
    pub last_good_available: bool,
    pub diagnostic: Option<Arc<str>>,
}

pub(super) fn import_issue_text(status: &AssetStatus) -> (Text, Text) {
    if *status == AssetStatus::Missing {
        (Text::AssetSourceMissing, Text::AssetRestoreSource)
    } else {
        (Text::AssetImportFailed, Text::AssetReplaceSource)
    }
}

pub(super) fn append_import_issues(
    page: &mut Value,
    issues: &BTreeMap<String, ImportIssue>,
    offset: usize,
) -> Result<(), &'static str> {
    let count = issues.len();
    if offset > count {
        return Err("issue_offset exceeds count");
    }
    let records: Vec<_> = issues
        .iter()
        .skip(offset)
        .take(ISSUE_PAGE_SIZE)
        .map(|(source, issue)| {
            let (message, recovery) = import_issue_text(&issue.status);
            let (source, source_truncated) = bounded_issue_text(source);
            let (diagnostic, diagnostic_truncated) = issue
                .diagnostic
                .as_deref()
                .map(bounded_issue_text)
                .map_or((None, false), |(text, truncated)| (Some(text), truncated));
            json!({
                "source": source, "source_truncated": source_truncated, "status": issue.status,
                "asset_id": issue.asset_id, "last_good_available": issue.last_good_available,
                "message": message.plain(), "recovery": recovery.plain(),
                "diagnostic": diagnostic, "diagnostic_truncated": diagnostic_truncated,
            })
        })
        .collect();
    page["issues"] = json!(records);
    page["issue_count"] = json!(count);
    page["issue_next_offset"] =
        json!((count - offset > ISSUE_PAGE_SIZE).then(|| (offset + ISSUE_PAGE_SIZE).to_string()));
    Ok(())
}

fn bounded_issue_text(text: &str) -> (&str, bool) {
    let end = text.floor_char_boundary(ISSUE_TEXT_BYTES);
    (&text[..end], end != text.len())
}

pub(super) fn page(store: &AssetStore, offset: usize) -> Result<Value, &'static str> {
    let count = store.infos().len();
    if offset > count {
        return Err("offset exceeds count");
    }
    let assets: Vec<_> = store
        .infos()
        .skip(offset)
        .take(PAGE_SIZE)
        .map(|info| {
            // Inspect only the bounded prefix: legacy OBJ errors can contain an
            // entire source directive. Keep the full diagnostic in the store.
            let (diagnostic, truncated) = match info.diagnostic.as_deref() {
                Some(text) => {
                    let end = text
                        .char_indices()
                        .nth(DIAGNOSTIC_CHARS)
                        .map(|(index, _)| index);
                    (Some(&text[..end.unwrap_or(text.len())]), end.is_some())
                }
                None => (None, false),
            };
            json!({
                "id": info.id, "kind": info.kind, "source": info.source,
                "hash": info.hash, "status": info.status,
                "mesh_normal_policy": info.mesh_normal_policy,
                "texture_role": info.texture_role,
                "diagnostic": diagnostic, "diagnostic_truncated": truncated,
            })
        })
        .collect();
    Ok(json!({
        "assets": assets, "asset_count": count,
        "asset_revision": store.revision().to_string(),
        "next_offset": if count - offset > PAGE_SIZE {
            Some((offset + PAGE_SIZE).to_string())
        } else { None },
    }))
}

/// Model source metadata only: never serialize node/material graphs into a page.
/// Clip summaries are catalog-bounded structure, never key times or values.
pub(super) fn model_summary(model: &rot_assets::ModelInfo) -> Value {
    let source: String = model.source.chars().take(DIAGNOSTIC_CHARS).collect();
    let clips: Vec<_> = model
        .clips
        .iter()
        .map(|(name, clip)| {
            // Tracks are bounded per clip, but a full page of models must stay small.
            let targets: Vec<_> = clip.targets.iter().take(PAGE_SIZE).collect();
            json!({"name":name,"id":clip.id,"duration":clip.duration,"targets":targets,
                "target_count":clip.targets.len(),"targets_truncated":clip.targets.len()>PAGE_SIZE})
        })
        .collect();
    let mut result = json!({"id":model.id,"source":source,"source_truncated":source.len()!=model.source.len(),
        "hash":model.hash,"status":"accepted","node_count":model.nodes.len(),"material_count":model.materials.len(),
        "clips":clips});
    let roughness_map_count = model
        .materials
        .values()
        .filter(|material| material.roughness_texture.is_some())
        .count();
    if roughness_map_count != 0 {
        result["roughness_map_count"] = json!(roughness_map_count);
    }
    let camera_count = model
        .nodes
        .values()
        .filter(|node| node.camera.is_some())
        .count();
    if camera_count != 0 {
        result["camera_count"] = json!(camera_count);
    }
    let light_count = model
        .nodes
        .values()
        .filter(|node| node.light.is_some())
        .count();
    if light_count != 0 {
        result["light_count"] = json!(light_count);
    }
    if !model.warnings.is_empty() {
        const PREVIEW_WARNINGS: usize = 4;
        let warnings: Vec<String> = model
            .warnings
            .iter()
            .filter(|message| message.contains("roughness map"))
            .chain(
                model
                    .warnings
                    .iter()
                    .filter(|message| !message.contains("roughness map")),
            )
            .take(PREVIEW_WARNINGS)
            .map(|message| message.chars().take(DIAGNOSTIC_CHARS).collect())
            .collect();
        result["warning_count"] = json!(model.warnings.len());
        result["warnings_truncated"] = json!(
            warnings.len() < model.warnings.len()
                || warnings
                    .iter()
                    .zip(&model.warnings)
                    .any(|(shown, full)| shown.len() < full.len())
        );
        result["warnings"] = json!(warnings);
    }
    result
}

pub(super) fn page_with_models(
    store: &AssetStore,
    offset: usize,
    model_offset: usize,
) -> Result<Value, &'static str> {
    let mut page = page(store, offset)?;
    let count = store.models().len();
    if model_offset > count {
        return Err("model_offset exceeds count");
    }
    let models: Vec<_> = store
        .models()
        .skip(model_offset)
        .take(PAGE_SIZE)
        .map(model_summary)
        .collect();
    let map = page.as_object_mut().expect("page object");
    map.insert("models".into(), json!(models));
    map.insert("model_count".into(), json!(count));
    map.insert(
        "model_next_offset".into(),
        json!(if count - model_offset > PAGE_SIZE {
            Some((model_offset + PAGE_SIZE).to_string())
        } else {
            None
        }),
    );
    Ok(page)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_issue_pages_are_ordered_and_bound_text_independently() {
        let root = tempfile::tempdir().unwrap();
        let store = AssetStore::open(root.path()).unwrap();
        let mut page = page_with_models(&store, 0, 0).unwrap();
        let issues = (0..9)
            .rev()
            .map(|index| {
                (
                    format!("Assets/{index}{}.obj", "é".repeat(200)),
                    ImportIssue {
                        status: AssetStatus::Invalid,
                        asset_id: None,
                        last_good_available: false,
                        diagnostic: Some("\0".repeat(100_000).into()),
                    },
                )
            })
            .collect();
        append_import_issues(&mut page, &issues, 0).unwrap();
        assert_eq!(page["issue_count"], 9);
        assert_eq!(page["issue_next_offset"], "4");
        assert_eq!(page["issues"].as_array().unwrap().len(), ISSUE_PAGE_SIZE);
        assert_eq!(page["issues"][0]["source_truncated"], true);
        assert_eq!(page["issues"][0]["diagnostic_truncated"], true);
        assert!(page["issues"][0]["source"]
            .as_str()
            .unwrap()
            .starts_with("Assets/0"));
        assert!(serde_json::to_vec(&page).unwrap().len() < 15_000);
        append_import_issues(&mut page, &issues, 4).unwrap();
        assert_eq!(page["issue_next_offset"], "8");
        assert!(page["issues"][0]["source"]
            .as_str()
            .unwrap()
            .starts_with("Assets/4"));
        append_import_issues(&mut page, &issues, 8).unwrap();
        assert_eq!(page["issue_next_offset"], Value::Null);
        assert_eq!(page["issues"].as_array().unwrap().len(), 1);
        assert!(page["issues"][0]["source"]
            .as_str()
            .unwrap()
            .starts_with("Assets/8"));
        append_import_issues(&mut page, &issues, 9).unwrap();
        assert_eq!(page["issues"], json!([]));
        assert!(append_import_issues(&mut page, &issues, 10).is_err());
        for field in ["assets", "models", "next_offset", "model_next_offset"] {
            assert_eq!(page[field], page_with_models(&store, 0, 0).unwrap()[field]);
        }
    }

    #[test]
    fn large_source_diagnostic_is_bounded_without_changing_accepted_assets() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("panel.obj");
        std::fs::write(&source, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let mut store = AssetStore::open(root.path()).unwrap();
        store.import("panel", "panel.obj").unwrap();
        let revision = store.revision();
        let accepted = store.mesh_snapshots()["panel"].clone();
        let invalid_source = "é".repeat(100_000);
        std::fs::write(&source, &invalid_source).unwrap();
        assert!(store.refresh("panel").is_err());
        let observation = page(&store, 0).unwrap();
        let record = &observation["assets"][0];
        assert_eq!(record["mesh_normal_policy"], "flat");
        assert_eq!(record["status"], "invalid");
        assert_eq!(record["diagnostic_truncated"], true);
        assert_eq!(
            record["diagnostic"].as_str().unwrap().chars().count(),
            DIAGNOSTIC_CHARS
        );
        assert!(serde_json::to_vec(&observation).unwrap().len() < 2048);
        assert!(
            store
                .infos()
                .next()
                .unwrap()
                .diagnostic
                .as_ref()
                .unwrap()
                .len()
                > 100_000
        );
        assert_eq!(store.revision(), revision);
        assert!(std::sync::Arc::ptr_eq(
            &accepted,
            &store.mesh_snapshots()["panel"]
        ));
        assert_eq!(std::fs::read_to_string(source).unwrap(), invalid_source);
        assert_eq!(page(&store, 1).unwrap()["assets"], json!([]));
        assert!(page(&store, 2).is_err());
    }
}
