use rot_assets::fixtures::{animated_glb, AnimatedFixture};
use rot_assets::{AssetStore, ClipProperty};
use std::fs;

fn catalog_version(root: &std::path::Path) -> u64 {
    let catalog: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(".rot-assets.json")).unwrap()).unwrap();
    catalog["version"].as_u64().unwrap()
}

// Unique boundary: clip structure is catalog identity, key data is a refreshable
// resource of the accepted generation, and both survive a fresh open.
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
