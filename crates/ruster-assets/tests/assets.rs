use ruster_assets::{parse_obj, AssetStatus, AssetStore, MAX_SOURCE_BYTES};
use std::fs;
use tempfile::TempDir;
const TRIANGLE: &str = "# owned triangle\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
fn project() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("mesh.obj"), TRIANGLE).unwrap();
    root
}

// The persistence boundary must survive process reopen and disposal/corruption
// of derived data without allowing changed sources to bypass explicit refresh.
#[test]
fn import_reopen_cache_rebuild_and_explicit_refresh() {
    let root = project();
    let mut store = AssetStore::open(root.path()).unwrap();
    let info = store.import_at(0, "triangle", "mesh.obj").unwrap();
    assert_eq!(info.status, AssetStatus::Ready);
    assert_eq!(store.revision(), 1);
    let original = store.mesh("triangle").unwrap().clone();
    let cache = root
        .path()
        .join(".ruster-cache/meshes")
        .join(format!("v3-{}.bin", info.hash.unwrap()));
    let valid_cache = fs::read(&cache).unwrap();
    assert_eq!(valid_cache.len(), 116 + 3 * 24 + 3 * 4);
    // Reject malformed structure, semantic-header mutations, payload corruption
    // and self-checksummed geometry outside the supported domain. A broken
    // source proves none of these corrupt caches supplies accepted geometry.
    for field in [
        "magic",
        "version",
        "source",
        "counts",
        "checksum",
        "coordinate",
        "truncated",
        "trailing",
    ] {
        let mut bytes = valid_cache.clone();
        match field {
            "magic" => bytes[0] ^= 1,
            "version" => bytes[8] ^= 1,
            "source" => bytes[20] ^= 1,
            "counts" => bytes[12..16].copy_from_slice(&u32::MAX.to_le_bytes()),
            "checksum" => bytes[116] ^= 1,
            "coordinate" => {
                use sha2::{Digest, Sha256};
                bytes[116..120].copy_from_slice(&1e9f32.to_le_bytes());
                let mut hasher = Sha256::new();
                hasher.update(&bytes[..84]);
                hasher.update(&bytes[116..]);
                bytes[84..116].copy_from_slice(&hasher.finalize());
            }
            "truncated" => {
                bytes.pop();
            }
            "trailing" => bytes.push(0),
            _ => unreachable!(),
        }
        fs::write(&cache, bytes).unwrap();
        fs::write(root.path().join("mesh.obj"), "broken source").unwrap();
        assert!(
            AssetStore::open(root.path())
                .unwrap()
                .mesh("triangle")
                .is_none(),
            "accepted {field}"
        );
        fs::write(root.path().join("mesh.obj"), TRIANGLE).unwrap();
        assert_eq!(
            AssetStore::open(root.path()).unwrap().mesh("triangle"),
            Some(&original)
        );
        assert_eq!(fs::read(&cache).unwrap(), valid_cache);
    }
    fs::remove_dir_all(root.path().join(".ruster-cache")).unwrap();
    let rebuilt = AssetStore::open(root.path()).unwrap();
    assert_eq!(rebuilt.mesh("triangle"), Some(&original));
    assert_eq!(rebuilt.revision(), 1);
    let changed = TRIANGLE.replace("v 1 0 0", "v 2 0 0");
    fs::write(root.path().join("mesh.obj"), changed).unwrap();
    let mut reopened = AssetStore::open(root.path()).unwrap();
    assert_eq!(reopened.inspect()[0].status, AssetStatus::Invalid);
    assert_eq!(reopened.mesh("triangle"), Some(&original));
    reopened.refresh_at(1, "triangle").unwrap();
    assert_eq!(reopened.revision(), 2);
    assert_ne!(reopened.mesh("triangle"), Some(&original));
    assert_eq!(
        AssetStore::open(root.path()).unwrap().meshes(),
        reopened.meshes()
    );
}

// The supported v1 format remains readable, including last-good recovery when
// v2/v3 are corrupt and the authoritative source is unavailable or malformed.
#[test]
fn legacy_cache_fallback_and_nondestructive_v3_publication() {
    use sha2::{Digest, Sha256};
    let root = project();
    let mut store = AssetStore::open(root.path()).unwrap();
    let info = store.import("triangle", "mesh.obj").unwrap();
    let hash = info.hash.unwrap();
    let original = store.mesh("triangle").unwrap().clone();
    let directory = root.path().join(".ruster-cache/meshes");
    let v3 = directory.join(format!("v3-{hash}.bin"));
    let v2 = directory.join(format!("v2-{hash}.bin"));
    // Construct a genuine historical v2 byte layout from untextured geometry:
    // its header/numeric payload match v3 without a UV stream, only version differs.
    let mut historical = fs::read(&v3).unwrap();
    historical[8..12].copy_from_slice(&2u32.to_le_bytes());
    let mut checksum = Sha256::new();
    checksum.update(&historical[..84]);
    checksum.update(&historical[116..]);
    historical[84..116].copy_from_slice(&checksum.finalize());
    fs::write(&v2, &historical).unwrap();
    fs::remove_file(&v3).unwrap();
    fs::remove_file(root.path().join("mesh.obj")).unwrap();
    assert_eq!(
        AssetStore::open(root.path()).unwrap().mesh("triangle"),
        Some(&original)
    );
    fs::write(root.path().join("mesh.obj"), TRIANGLE).unwrap();
    let v1 = directory.join(format!("v1-{hash}.json"));
    let legacy = serde_json::to_vec(&serde_json::json!({
        "format": "ruster-mesh", "version": 1, "source_hash": hash,
        "mesh_hash": format!("{:x}", Sha256::digest(serde_json::to_vec(&original).unwrap())),
        "mesh": original,
    }))
    .unwrap();
    fs::write(&v1, &legacy).unwrap();
    fs::write(&v2, "broken current cache").unwrap();
    fs::remove_file(root.path().join("mesh.obj")).unwrap();
    let missing = AssetStore::open(root.path()).unwrap();
    assert_eq!(missing.mesh("triangle"), Some(&original));
    assert_eq!(missing.inspect()[0].status, AssetStatus::Missing);
    fs::write(root.path().join("mesh.obj"), "v NaN 0 0").unwrap();
    let invalid = AssetStore::open(root.path()).unwrap();
    assert_eq!(invalid.mesh("triangle"), Some(&original));
    assert_eq!(invalid.inspect()[0].status, AssetStatus::Invalid);
    // Unsupported/corrupt legacy data cannot supply last-good geometry either.
    fs::write(&v1, legacy.as_slice().get(..legacy.len() - 1).unwrap()).unwrap();
    assert!(AssetStore::open(root.path())
        .unwrap()
        .mesh("triangle")
        .is_none());
    fs::write(&v1, &legacy).unwrap();
    fs::write(root.path().join("mesh.obj"), TRIANGLE).unwrap();
    store.refresh("triangle").unwrap();
    assert_eq!(store.revision(), 1);
    assert_eq!(fs::read(&v1).unwrap(), legacy);
    assert!(fs::read(&v3).unwrap().starts_with(b"RUSTMESH"));
    assert_eq!(fs::read(&v2).unwrap(), b"broken current cache");
    assert_eq!(
        AssetStore::open(root.path()).unwrap().mesh("triangle"),
        Some(&original)
    );
}

// Immutable snapshots identify exactly the accepted meshes that changed; a
// caller can retain GPU resources for all other entries without geometry copies.
#[test]
fn snapshots_preserve_identity_except_for_successful_content_change() {
    use std::sync::Arc;
    let root = project();
    fs::write(root.path().join("other.obj"), TRIANGLE).unwrap();
    let mut store = AssetStore::open(root.path()).unwrap();
    store.import("one", "mesh.obj").unwrap();
    let first = store.mesh_snapshots();
    store.import("two", "other.obj").unwrap();
    let both = store.mesh_snapshots();
    assert!(Arc::ptr_eq(&first["one"], &both["one"]));
    store.refresh("two").unwrap();
    assert!(Arc::ptr_eq(&both["two"], &store.mesh_snapshots()["two"]));
    store.rename("two", "renamed.obj").unwrap();
    assert!(Arc::ptr_eq(&both["two"], &store.mesh_snapshots()["two"]));
    fs::write(
        root.path().join("renamed.obj"),
        format!("{TRIANGLE}# source-only edit\n"),
    )
    .unwrap();
    let revision = store.revision();
    store.refresh("two").unwrap();
    assert_eq!(store.revision(), revision + 1);
    assert!(Arc::ptr_eq(&both["two"], &store.mesh_snapshots()["two"]));
    fs::write(root.path().join("renamed.obj"), "broken source").unwrap();
    assert!(store.refresh("two").is_err());
    assert!(Arc::ptr_eq(&both["two"], &store.mesh_snapshots()["two"]));
    fs::write(
        root.path().join("renamed.obj"),
        TRIANGLE.replace("v 1 0 0", "v 2 0 0"),
    )
    .unwrap();
    store.refresh("two").unwrap();
    let changed = store.mesh_snapshots();
    assert!(Arc::ptr_eq(&both["one"], &changed["one"]));
    assert!(!Arc::ptr_eq(&both["two"], &changed["two"]));
    assert_eq!(both["two"].vertices[1].position, [1.0, 0.0, 0.0]);
    assert_eq!(store.meshes()["two"], *changed["two"]);
}

// Broken/missing source diagnostics must not destroy the last accepted mesh or
// advance the accepted revision, and valid edits must recover without reimport.
#[test]
fn invalid_missing_source_preserves_good_mesh_and_recovers() {
    let root = project();
    let mut store = AssetStore::open(root.path()).unwrap();
    let info = store.import("triangle", "mesh.obj").unwrap();
    let original = store.mesh("triangle").unwrap().clone();
    fs::write(root.path().join("mesh.obj"), "v NaN 0 0").unwrap();
    let failure = store.refresh("triangle").unwrap_err().to_string();
    assert!(failure.contains("line 1"));
    assert_eq!(store.inspect()[0].hash, info.hash);
    assert_eq!(store.revision(), 1);
    assert_eq!(store.mesh("triangle"), Some(&original));
    fs::remove_file(root.path().join("mesh.obj")).unwrap();
    let missing = AssetStore::open(root.path()).unwrap();
    assert_eq!(missing.inspect()[0].status, AssetStatus::Missing);
    assert_eq!(missing.mesh("triangle"), Some(&original));
    fs::write(root.path().join("mesh.obj"), TRIANGLE).unwrap();
    store.refresh("triangle").unwrap();
    assert_eq!(store.inspect()[0].status, AssetStatus::Ready);
    assert_eq!(store.revision(), 1); // same accepted content, no catalog mutation
}

// Rename is an actual source move under a stable ID, never an overwrite.
#[test]
fn source_rename_preserves_identity_and_refuses_overwrite() {
    let root = project();
    let mut store = AssetStore::open(root.path()).unwrap();
    let before = store.import("triangle", "mesh.obj").unwrap();
    assert!(store
        .import("alias", "mesh.obj")
        .unwrap_err()
        .to_string()
        .contains("already registered"));
    assert_eq!(store.revision(), 1);
    fs::write(root.path().join("occupied.obj"), TRIANGLE).unwrap();
    assert!(store.rename("triangle", "occupied.obj").is_err());
    let after = store.rename_at(1, "triangle", "renamed.obj").unwrap();
    assert_eq!(after.id, before.id);
    assert_eq!(after.hash, before.hash);
    assert_eq!(after.source, "renamed.obj");
    assert!(!root.path().join("mesh.obj").exists());
    assert!(root.path().join("renamed.obj").exists());
    assert_eq!(
        AssetStore::open(root.path()).unwrap().inspect(),
        store.inspect()
    );
    assert_eq!(store.revision(), 2);
}

// Stale API tokens and cooperating cross-process catalog edits are separate
// conflicts: compare all baseline bytes, not just revision or metadata length.
#[test]
fn stale_revision_and_full_catalog_baseline_conflict() {
    let root = project();
    let mut first = AssetStore::open(root.path()).unwrap();
    let mut second = AssetStore::open(root.path()).unwrap();
    first.import("one", "mesh.obj").unwrap();
    assert!(second
        .import("two", "mesh.obj")
        .unwrap_err()
        .to_string()
        .contains("changed on disk"));
    assert!(first
        .refresh_at(0, "one")
        .unwrap_err()
        .to_string()
        .contains("revision conflict"));
    let path = root.path().join(".ruster-assets.json");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("mesh.obj", "fake.obj");
    fs::write(path, text).unwrap();
    assert!(first
        .refresh("one")
        .unwrap_err()
        .to_string()
        .contains("changed on disk"));
}

// Cache publication failure must retain the accepted in-memory mesh and disk
// catalog; an explicit later retry can recover without reopening the session.
#[test]
fn failed_cache_publish_keeps_accepted_state() {
    let root = project();
    let mut store = AssetStore::open(root.path()).unwrap();
    let before = store.import("one", "mesh.obj").unwrap();
    let mesh = store.mesh("one").unwrap().clone();
    let catalog = fs::read(root.path().join(".ruster-assets.json")).unwrap();
    fs::write(
        root.path().join("mesh.obj"),
        TRIANGLE.replace("v 1 0 0", "v 2 0 0"),
    )
    .unwrap();
    fs::remove_dir_all(root.path().join(".ruster-cache")).unwrap();
    fs::write(root.path().join(".ruster-cache"), "blocking file").unwrap();
    assert!(store.refresh("one").is_err());
    assert_eq!(store.inspect()[0].hash, before.hash);
    assert_eq!(store.mesh("one"), Some(&mesh));
    assert_eq!(store.revision(), 1);
    assert_eq!(
        fs::read(root.path().join(".ruster-assets.json")).unwrap(),
        catalog
    );
    fs::remove_file(root.path().join(".ruster-cache")).unwrap();
    store.refresh("one").unwrap();
    assert_eq!(store.revision(), 2);
}

#[cfg(unix)]
#[test]
fn catalog_publish_failure_rolls_back_source_rename() {
    use std::os::unix::fs::PermissionsExt;
    let root = project();
    fs::create_dir(root.path().join("sources")).unwrap();
    fs::rename(
        root.path().join("mesh.obj"),
        root.path().join("sources/mesh.obj"),
    )
    .unwrap();
    let mut store = AssetStore::open(root.path()).unwrap();
    store.import("one", "sources/mesh.obj").unwrap();
    let catalog = fs::read(root.path().join(".ruster-assets.json")).unwrap();
    // Source directory remains writable, catalog parent cannot create temp file.
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o555)).unwrap();
    let result = store.rename("one", "sources/moved.obj");
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err());
    assert!(root.path().join("sources/mesh.obj").exists());
    assert!(!root.path().join("sources/moved.obj").exists());
    assert_eq!(store.inspect()[0].source, "sources/mesh.obj");
    assert_eq!(store.revision(), 1);
    assert_eq!(
        fs::read(root.path().join(".ruster-assets.json")).unwrap(),
        catalog
    );
}

// Source paths, cache paths and registry records must stay bounded/project-local.
#[test]
fn paths_and_registry_versions_fail_closed() {
    let root = project();
    let mut store = AssetStore::open(root.path()).unwrap();
    for path in [
        "../mesh.obj",
        "/tmp/mesh.obj",
        "./mesh.obj",
        ".ruster-cache/mesh.obj",
        "mesh.txt",
        "a/../../mesh.obj",
        "x\\mesh.obj",
    ] {
        assert!(store.import("one", path).is_err(), "accepted {path}");
    }
    assert!(store.import("invalid id", "mesh.obj").is_err());
    assert!(store.inspect().is_empty());
    fs::write(
        root.path().join(".ruster-assets.json"),
        r#"{"format":"ruster-assets","version":5,"revision":0,"assets":{}}"#,
    )
    .unwrap();
    assert!(AssetStore::open(root.path())
        .unwrap_err()
        .to_string()
        .contains("version"));
    fs::write(root.path().join(".ruster-assets.json"), r#"{"format":"ruster-assets","version":1,"revision":0,"assets":{"one":{"source":"mesh.obj","hash":null},"two":{"source":"mesh.obj","hash":null}}}"#).unwrap();
    assert!(AssetStore::open(root.path())
        .unwrap_err()
        .to_string()
        .contains("duplicate registered source"));
}
#[cfg(unix)]
#[test]
fn symlink_sources_and_cache_are_rejected() {
    use std::os::unix::fs::symlink;
    let root = project();
    let outside = project();
    symlink(outside.path(), root.path().join("outside")).unwrap();
    symlink(root.path().join("mesh.obj"), root.path().join("alias.obj")).unwrap();
    let mut store = AssetStore::open(root.path()).unwrap();
    assert!(store.import("one", "outside/mesh.obj").is_err());
    assert!(store.import("one", "alias.obj").is_err());
    symlink(outside.path(), root.path().join(".ruster-cache")).unwrap();
    assert!(store.import("one", "mesh.obj").is_err());
    assert!(store.inspect().is_empty());
}

// The parser accepts representative Blender-like index syntax, but rejects
// unsupported semantics, malformed/degenerate geometry and resource excess.
#[test]
fn obj_index_and_format_boundaries() {
    let indexed = "o Mesh\nv 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nvn 0 0 1\ns off\nf -3/1/1 -2/2/1 -1/3/1\n";
    let mesh = parse_obj(indexed.as_bytes()).unwrap();
    assert_eq!(mesh.vertices.len(), 3);
    assert_eq!(mesh.vertices[0].normal, [0.0, 0.0, 1.0]);
    assert_eq!(
        mesh.texcoords,
        Some(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]])
    );
    for bad in [
        TRIANGLE.replace("f 1 2 3", "f 0 2 3"),
        TRIANGLE.replace("f 1 2 3", "f 1 2 4"),
        TRIANGLE.replace("f 1 2 3", "f 1 2 2"),
        TRIANGLE.replace("f 1 2 3", "f 1 2 3 1"),
        TRIANGLE.replace("f 1 2 3", "f 1/ 2/ 3/"),
        TRIANGLE.replace("v 1 0 0", "v inf 0 0"),
        format!("mtllib materials.mtl\n{TRIANGLE}"),
        format!("usemtl paint\n{TRIANGLE}"),
        format!("curv 0 1 1 2\n{TRIANGLE}"),
    ] {
        assert!(parse_obj(bad.as_bytes()).is_err(), "accepted {bad}");
    }
    assert!(parse_obj(&vec![b' '; MAX_SOURCE_BYTES + 1]).is_err());

    // Policy is explicit: legacy callers retain flat normals, authored mode
    // preserves per-corner seams and normalizes even subnormal supplied values.
    use ruster_assets::{parse_obj_with_normal_policy, MeshNormalPolicy};
    let authored =
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 2 0\nvn 1e-40 0 0\nf 1//1 2//-1 3//1\nf 1//2 3//1 2//2\n";
    let smooth =
        parse_obj_with_normal_policy(authored.as_bytes(), MeshNormalPolicy::AuthoredWhenComplete)
            .unwrap();
    assert_eq!(smooth.vertices[0].normal, [0., 1., 0.]);
    assert_eq!(smooth.vertices[1].normal, [1., 0., 0.]);
    assert_eq!(smooth.vertices[3].position, smooth.vertices[0].position);
    assert_ne!(smooth.vertices[3].normal, smooth.vertices[0].normal);
    assert_eq!(
        parse_obj(authored.as_bytes()).unwrap().vertices[0].normal,
        [0., 0., 1.]
    );
    assert_eq!(
        parse_obj_with_normal_policy(TRIANGLE.as_bytes(), MeshNormalPolicy::AuthoredWhenComplete)
            .unwrap(),
        parse_obj(TRIANGLE.as_bytes()).unwrap()
    );
    for bad in [
        authored.replace("2//-1", "2"),
        authored.replace("f 1//2 3//1 2//2", "f 1 3 2"),
        authored.replace("vn 0 2 0", "vn 0 0 0"),
        authored.replace("vn 0 2 0", "vn NaN 0 0"),
        authored.replace("1//1", "1//3"),
    ] {
        assert!(
            parse_obj_with_normal_policy(bad.as_bytes(), MeshNormalPolicy::AuthoredWhenComplete)
                .is_err(),
            "accepted {bad}"
        );
    }
    assert!(parse_obj(authored.replace("vn 0 2 0", "vn 0 0 0").as_bytes()).is_ok());
}

// Both per-project asset count and aggregate resolved geometry are enforced
// before committing another accepted entry, including on process reopen.
#[test]
fn catalog_and_resolved_payload_limits_preserve_previous_state() {
    let root = project();
    let mut small = AssetStore::open(root.path()).unwrap();
    for i in 0..ruster_assets::MAX_ASSETS {
        let source = format!("small-{i}.obj");
        fs::copy(root.path().join("mesh.obj"), root.path().join(&source)).unwrap();
        small.import(&format!("small-{i}"), &source).unwrap();
    }
    assert!(small.import("overflow", "mesh.obj").is_err());
    assert_eq!(small.revision(), ruster_assets::MAX_ASSETS as u64);
    let root = project();
    let mut source = String::from("v 0 0 0\nv 1 0 0\nv 0 1 0\n");
    source.push_str(&"f 1 2 3\n".repeat(32_768));
    fs::write(root.path().join("mesh.obj"), source).unwrap();
    let mut store = AssetStore::open(root.path()).unwrap();
    let per_mesh =
        98_304 * (std::mem::size_of::<ruster_assets::Vertex>() + std::mem::size_of::<u32>());
    let accepted = ruster_assets::MAX_RESOLVED_BYTES / per_mesh;
    for i in 0..accepted {
        let source = format!("large-{i}.obj");
        fs::copy(root.path().join("mesh.obj"), root.path().join(&source)).unwrap();
        store.import(&format!("large-{i}"), &source).unwrap();
    }
    assert!(store
        .import("overflow", "mesh.obj")
        .unwrap_err()
        .to_string()
        .contains("48 MiB"));
    assert_eq!(store.inspect().len(), accepted);
    assert_eq!(store.revision(), accepted as u64);
    assert_eq!(
        AssetStore::open(root.path()).unwrap().meshes().len(),
        accepted
    );
}
