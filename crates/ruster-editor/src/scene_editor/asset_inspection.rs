//! Bounded borrowed catalog observations for the native bridge.
use ruster_assets::AssetStore;
use serde_json::{json, Value};

const PAGE_SIZE: usize = 16;
const DIAGNOSTIC_CHARS: usize = 256;

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

#[cfg(test)]
mod tests {
    use super::*;

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
