use ruster_assets::{parse_obj_with_normal_policy, AssetStore, MeshNormalPolicy, TextureRole};
use std::{collections::BTreeSet, fs, path::PathBuf, sync::Arc};

#[test]
#[ignore = "requires the pinned Blender GLB A/B/reordered/removed/malformed fixtures"]
fn real_glb_catalog_refresh_and_reopen() {
    let fixture = PathBuf::from(std::env::var_os("RUSTER_TEST_GLB_FIXTURES").unwrap());
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("beacon.glb");
    fs::copy(fixture.join("beacon-A.glb"), &source).unwrap();
    let mut store = AssetStore::open(root.path()).unwrap();
    let first = store.import_glb_at(0, "beacon", "beacon.glb").unwrap();
    assert_eq!(store.revision(), 1);
    assert_eq!(first.nodes.len(), 3);
    let assembly = &first.nodes["BeaconAssembly"];
    assert_eq!(assembly.translation, [1.25, 0.75, -0.5]);
    assert!((assembly.rotation[1] - 0.35).abs() < 1e-6);
    let housing = &first.nodes["BeaconAssembly/BeaconCap"];
    assert_eq!(housing.parent.as_deref(), Some("BeaconAssembly"));
    let mesh_id = housing.mesh_id.as_ref().unwrap();
    let material = &first.materials[housing.material.as_ref().unwrap()];
    assert_eq!(material.roughness, 0.6);
    assert!(material.linear && material.repeat);
    let texture_id = material.base_color_texture.as_ref().unwrap();
    let first_mesh = store.mesh_snapshots()[mesh_id].clone();
    let first_texture = store.texture_snapshots()[texture_id].clone();
    let first_catalog = store
        .export_snapshot(&BTreeSet::from([mesh_id.clone(), texture_id.clone()]))
        .unwrap();
    assert!(first_mesh.has_complete_uvs());
    assert_eq!(first_texture.role, TextureRole::BaseColor);

    fs::copy(fixture.join("beacon-B-reordered.glb"), &source).unwrap();
    let second = store.refresh_glb_at(1, "beacon").unwrap();
    assert_eq!(store.revision(), 2);
    assert_ne!(first.hash, second.hash);
    assert_eq!(first.nodes, second.nodes);
    assert_eq!(first.materials, second.materials);
    let second_mesh = store.mesh_snapshots()[mesh_id].clone();
    let second_texture = store.texture_snapshots()[texture_id].clone();
    assert_ne!(first_mesh, second_mesh);
    assert_ne!(first_texture, second_texture);
    assert!(first_catalog.read_sources().is_err());
    let accepted = fs::read(root.path().join(".ruster-assets.json")).unwrap();
    let export = store
        .export_snapshot(&BTreeSet::from([mesh_id.clone(), texture_id.clone()]))
        .unwrap();
    assert_eq!(
        export.mesh_normal_policy(mesh_id),
        Some(MeshNormalPolicy::AuthoredWhenComplete)
    );
    assert_eq!(
        export.texture_role(texture_id),
        Some(TextureRole::BaseColor)
    );
    let sources = export.read_sources().unwrap();
    assert_eq!(
        parse_obj_with_normal_policy(&sources[mesh_id], MeshNormalPolicy::AuthoredWhenComplete)
            .unwrap(),
        *second_mesh
    );
    assert_eq!(
        ruster_assets::decode_png(&sources[texture_id]).unwrap(),
        *second_texture
    );

    for (name, expected) in [
        ("beacon-malformed.glb", "GLB 2.0 container"),
        ("beacon-removed.glb", "structure changed"),
    ] {
        fs::copy(fixture.join(name), &source).unwrap();
        let failure = store.refresh_glb_at(2, "beacon").unwrap_err();
        assert!(failure.to_string().contains(expected), "{failure}");
        assert_eq!(store.revision(), 2);
        assert_eq!(
            fs::read(root.path().join(".ruster-assets.json")).unwrap(),
            accepted
        );
        assert_eq!(store.model("beacon"), Some(&second));
        assert!(Arc::ptr_eq(&store.mesh_snapshots()[mesh_id], &second_mesh));
        assert!(Arc::ptr_eq(
            &store.texture_snapshots()[texture_id],
            &second_texture
        ));
        // A new store can reconstruct the same accepted resources with a rejected
        // authored path and no derived-cache directory.
        assert!(!root.path().join(".ruster-cache").exists());
        let reopened = AssetStore::open(root.path()).unwrap();
        assert_eq!(reopened.model("beacon"), Some(&second));
        assert_eq!(reopened.mesh(mesh_id), Some(second_mesh.as_ref()));
        assert_eq!(reopened.texture(texture_id), Some(second_texture.as_ref()));
        assert_eq!(export.read_sources().unwrap(), sources);
    }
}

#[test]
#[ignore = "requires pinned Blender GLB A/B fixtures; actual guarded publish I/O failure"]
fn prepared_glb_publish_failure_preserves_the_active_generation() {
    use ruster_assets::{AssetMutation, AssetOutcome};
    use sha2::{Digest, Sha256};

    let fixture = PathBuf::from(std::env::var_os("RUSTER_TEST_GLB_FIXTURES").unwrap());
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("beacon.glb");
    fs::copy(fixture.join("beacon-A.glb"), &source).unwrap();
    let mut store = AssetStore::open(root.path()).unwrap();
    let first = store.import_glb_at(0, "beacon", "beacon.glb").unwrap();
    let old_meshes = store.mesh_snapshots();
    let old_textures = store.texture_snapshots();
    let catalog = root.path().join(".ruster-assets.json");
    let accepted = fs::read(&catalog).unwrap();
    let candidate = fs::read(fixture.join("beacon-B-reordered.glb")).unwrap();
    let candidate_hash = format!("{:x}", Sha256::digest(&candidate));
    fs::write(&source, candidate).unwrap();
    let request = || AssetMutation::RefreshGlb {
        id: "beacon".into(),
    };
    let prepared = store.request_at(1, request()).unwrap().prepare();
    let destination = root
        .path()
        .join(".ruster-models")
        .join(format!("{candidate_hash}.glb"));
    assert!(
        !destination.exists(),
        "prepare must not publish a generation"
    );
    fs::create_dir(&destination).unwrap();

    let completion = prepared.publish();
    assert!(!completion.catalog_published());
    let adoption = store.adopt(completion).unwrap();
    assert!(adoption
        .result
        .unwrap_err()
        .to_string()
        .contains("write destination must be a regular file"));
    assert_eq!(store.revision(), 1);
    assert_eq!(fs::read(&catalog).unwrap(), accepted);
    assert_eq!(store.model("beacon"), Some(&first));
    for (id, mesh) in &old_meshes {
        assert!(Arc::ptr_eq(mesh, &store.mesh_snapshots()[id]));
    }
    for (id, texture) in &old_textures {
        assert!(Arc::ptr_eq(texture, &store.texture_snapshots()[id]));
    }
    assert!(!root.path().join(".ruster-cache").exists());
    let reopened = AssetStore::open(root.path()).unwrap();
    assert_eq!(reopened.model("beacon"), Some(&first));
    assert_eq!(reopened.mesh_snapshots(), old_meshes);
    assert_eq!(reopened.texture_snapshots(), old_textures);

    fs::remove_dir(&destination).unwrap();
    let completion = store.request_at(1, request()).unwrap().prepare().publish();
    assert!(completion.catalog_published());
    let AssetOutcome::Model(second) = store.adopt(completion).unwrap().result.unwrap() else {
        panic!("static GLB publication must return a model identity");
    };
    assert_eq!(store.revision(), 2);
    assert_eq!(second.hash, candidate_hash);
    assert_eq!(second.nodes, first.nodes);
    assert_eq!(second.materials, first.materials);
    assert_ne!(store.mesh_snapshots(), old_meshes);
    assert_ne!(store.texture_snapshots(), old_textures);
    assert_eq!(
        AssetStore::open(root.path()).unwrap().model("beacon"),
        Some(second.as_ref())
    );
}
