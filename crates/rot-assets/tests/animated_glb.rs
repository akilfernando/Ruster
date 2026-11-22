use rot_assets::fixtures::{animated_glb, AnimatedFixture};
use rot_assets::{AssetStore, ClipProperty};
use std::fs;

fn catalog_version(root: &std::path::Path) -> u64 {
    let catalog: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(".rot-assets.json")).unwrap()).unwrap();
    catalog["version"].as_u64().unwrap()
}

#[test]
fn unsupported_material_settings_import_with_persistent_warnings() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("rig.glb"),
        animated_glb(AnimatedFixture {
            material_fallbacks: true,
            ..Default::default()
        }),
    )
    .unwrap();
    let mut store = AssetStore::open(root.path()).unwrap();
    let model = store.import_glb_at(0, "rig", "rig.glb").unwrap();

    assert_eq!(model.materials["Material 1"].roughness, 0.08);
    assert_eq!(
        model.nodes["Rig/Arm"].material.as_deref(),
        Some("Material 1")
    );
    assert!(model.clips.contains_key("Swing"));
    assert_eq!(
        model.warnings,
        ["Material 1: alpha rendered opaque, double-sided surface rendered single-sided, metallic factor rendered as dielectric, roughness raised to 0.08, emissive factor omitted"]
    );
    assert_eq!(
        AssetStore::open(root.path()).unwrap().model("rig"),
        Some(&model)
    );
}

#[test]
fn split_primitives_keep_surface_ids_when_material_slots_reorder() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("rig.glb");
    let fixture = AnimatedFixture {
        split_materials: true,
        ..Default::default()
    };
    fs::write(&source, animated_glb(fixture)).unwrap();
    let mut store = AssetStore::open(root.path()).unwrap();
    let first = store.import_glb_at(0, "rig", "rig.glb").unwrap();
    assert!(first.warnings.is_empty());

    assert_eq!(first.nodes.len(), 7);
    assert_eq!(first.nodes["Rig/Arm"].mesh_id, None);
    let surfaces: Vec<_> = first
        .nodes
        .values()
        .filter(|node| node.parent.as_deref() == Some("Rig/Arm"))
        .collect();
    assert_eq!(
        surfaces
            .iter()
            .map(|node| node.material.as_deref())
            .collect::<Vec<_>>(),
        [Some("Accent"), Some("Paint")]
    );
    let ids: std::collections::BTreeSet<_> = surfaces
        .iter()
        .map(|node| node.mesh_id.clone().unwrap())
        .collect();
    assert_eq!(ids.len(), 2);
    assert!(first.clips["Swing"]
        .targets
        .iter()
        .all(|target| target.node == "Rig/Arm"));
    let snapshots = store.mesh_snapshots();
    let packaged = store.export_snapshot(&ids).unwrap().read_sources().unwrap();
    assert_eq!(packaged.len(), 2);

    fs::write(
        &source,
        animated_glb(AnimatedFixture {
            reverse_primitives: true,
            ..fixture
        }),
    )
    .unwrap();
    let reordered = store.refresh_glb_at(1, "rig").unwrap();
    assert_ne!(first.hash, reordered.hash);
    assert_eq!(first.nodes, reordered.nodes);
    assert_eq!(first.materials, reordered.materials);
    assert_eq!(first.clips, reordered.clips);
    assert_eq!(snapshots, store.mesh_snapshots());
    let reopened = AssetStore::open(root.path()).unwrap();
    assert_eq!(reopened.model("rig"), Some(&reordered));
}

#[test]
fn repeated_animated_nodes_target_the_source_node_instead_of_split_primitives() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("rig.glb");
    fs::write(
        &source,
        animated_glb(AnimatedFixture {
            sibling_name: "Arm",
            split_materials: true,
            ..Default::default()
        }),
    )
    .unwrap();
    let mut store = AssetStore::open(root.path()).unwrap();
    let accepted = store.import_glb_at(0, "rig", "rig.glb").unwrap();
    let clip = &accepted.clips["Swing"];
    assert_eq!(accepted.nodes["Rig/ node-1"].name, "Arm");
    assert_eq!(accepted.nodes["Rig/ node-2"].name, "Arm");
    for target in &clip.targets {
        assert_eq!(target.node, "Rig/ node-1");
        assert!(!rot_assets::ModelInfo::is_primitive_path(&target.node));
    }
    assert!(accepted.nodes.keys().any(|path| {
        path.starts_with("Rig/ node-1/") && rot_assets::ModelInfo::is_primitive_path(path)
    }));
    let reopened = AssetStore::open(root.path()).unwrap();
    assert_eq!(reopened.model("rig"), Some(&accepted));
    assert_eq!(reopened.clip(&clip.id), store.clip(&clip.id));
}

#[test]
fn clip_import_refresh_and_reopen_keep_stable_identity() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("rig.glb");
    fs::write(&source, animated_glb(AnimatedFixture::default())).unwrap();
    let mut store = AssetStore::open(root.path()).unwrap();
    let first = store.import_glb_at(0, "rig", "rig.glb").unwrap();
    assert_eq!(catalog_version(root.path()), 7);
    let clip = &first.clips["Swing"];
    assert_eq!(clip.duration, 2.0);
    assert_eq!(
        clip.targets
            .iter()
            .map(|target| (target.node.as_str(), target.property))
            .collect::<Vec<_>>(),
        [
            ("Rig/Arm", ClipProperty::Translation),
            ("Rig/Arm", ClipProperty::Rotation),
        ]
    );
    let first_bytes = store.clip(&clip.id).unwrap().clone();
    let payload: serde_json::Value = serde_json::from_slice(&first_bytes).unwrap();
    assert_eq!(payload["format"], "rot-clip");
    assert_eq!(
        payload["tracks"][1]["times"],
        serde_json::json!([0.0, 1.0, 2.0])
    );
    assert_eq!(store.clip_owner(&clip.id).unwrap().1, "Swing");

    // Changed keys are an ordinary Refresh with the same clip resource ID.
    fs::write(
        &source,
        animated_glb(AnimatedFixture {
            mid_angle: 0.5,
            end_x: 3.0,
            ..Default::default()
        }),
    )
    .unwrap();
    let revised = store.refresh_glb_at(1, "rig").unwrap();
    assert_eq!(revised.clips["Swing"].id, clip.id);
    assert_ne!(revised.hash, first.hash);
    assert_ne!(store.clip(&clip.id).unwrap(), &first_bytes);
    let accepted = store.clip(&clip.id).unwrap().clone();

    // A renamed clip or changed animated property is structural.
    for changed in [
        AnimatedFixture {
            clip_name: "Wave",
            ..Default::default()
        },
        AnimatedFixture {
            rotation_path: "scale",
            ..Default::default()
        },
    ] {
        fs::write(&source, animated_glb(changed)).unwrap();
        let refused = store.refresh_glb_at(2, "rig").unwrap_err().to_string();
        assert!(
            refused.contains("animation structure changed") || refused.contains("VEC3"),
            "{refused}"
        );
        assert_eq!(store.clip(&clip.id).unwrap(), &accepted);
    }
    drop(store);

    let reopened = AssetStore::open(root.path()).unwrap();
    assert_eq!(reopened.model("rig").unwrap(), &revised);
    assert_eq!(reopened.clip(&clip.id).unwrap(), &accepted);
}

#[test]
fn unsupported_animation_sources_refuse_before_publication() {
    for (fixture, diagnostic) in [
        (
            AnimatedFixture {
                rotation_interpolation: "CUBICSPLINE",
                ..Default::default()
            },
            "cubic spline interpolation is unsupported",
        ),
        (
            AnimatedFixture {
                rotation_path: "weights",
                ..Default::default()
            },
            "morph target weights are unsupported",
        ),
        (
            AnimatedFixture {
                clip_name: "bad/name",
                ..Default::default()
            },
            "animation clips require a unique",
        ),
        (
            AnimatedFixture {
                end_x: 20_000.0,
                ..Default::default()
            },
            "translation within ±10000",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("rig.glb"), animated_glb(fixture)).unwrap();
        let mut store = AssetStore::open(root.path()).unwrap();
        let failure = store.import_glb_at(0, "rig", "rig.glb").unwrap_err();
        assert!(failure.to_string().contains(diagnostic), "{failure}");
        assert!(!root.path().join(".rot-assets.json").exists());
        assert!(!root.path().join(".rot-models").exists());
    }
}

// Real exporter boundary: Blender samples every frame and starts keys at frame
// one, which the hand-built fixture does not exercise.
#[test]
#[ignore = "requires Blender beacon-anim-A/B.glb in ROT_TEST_ANIMATED_GLB_FIXTURES"]
fn blender_sampled_clip_imports_and_refreshes() {
    let fixtures = std::path::PathBuf::from(
        std::env::var_os("ROT_TEST_ANIMATED_GLB_FIXTURES").expect("fixture directory"),
    );
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("beacon.glb");
    fs::copy(fixtures.join("beacon-anim-A.glb"), &source).unwrap();
    let mut store = AssetStore::open(root.path()).unwrap();
    let first = store.import_glb_at(0, "beacon", "beacon.glb").unwrap();
    let clip = &first.clips["CapSpin"];
    assert_eq!(
        clip.targets
            .iter()
            .map(|target| (target.node.as_str(), target.property))
            .collect::<Vec<_>>(),
        [
            ("BeaconAssembly/BeaconCap", ClipProperty::Translation),
            ("BeaconAssembly/BeaconCap", ClipProperty::Rotation),
        ]
    );
    assert!(
        (clip.duration - 49.0 / 24.0).abs() < 1e-5,
        "{}",
        clip.duration
    );
    let payload: serde_json::Value = serde_json::from_slice(store.clip(&clip.id).unwrap()).unwrap();
    for track in payload["tracks"].as_array().unwrap() {
        assert_eq!(track["times"].as_array().unwrap().len(), 49);
        assert_eq!(track["interpolation"], "linear");
    }
    let first_bytes = store.clip(&clip.id).unwrap().clone();
    fs::copy(fixtures.join("beacon-anim-B.glb"), &source).unwrap();
    let revised = store.refresh_glb_at(1, "beacon").unwrap();
    assert_eq!(revised.clips["CapSpin"].id, clip.id);
    assert_ne!(store.clip(&clip.id).unwrap(), &first_bytes);
    println!(
        "blender clip {} duration {} payload {} bytes",
        clip.id,
        clip.duration,
        first_bytes.len()
    );
}
