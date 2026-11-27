use rot_assets::{decode_packaged_mesh, AssetStore, MeshNormalPolicy, TextureRole};
use std::{collections::BTreeSet, fs, path::PathBuf, sync::Arc};

#[test]
#[ignore = "requires the pinned Blender GLB A/B/reordered/removed/malformed fixtures"]
fn real_glb_catalog_refresh_and_reopen() {
    let fixture = PathBuf::from(std::env::var_os("ROT_TEST_GLB_FIXTURES").unwrap());
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
    let accepted = fs::read(root.path().join(".rot-assets.json")).unwrap();
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
        decode_packaged_mesh(&sources[mesh_id], MeshNormalPolicy::AuthoredWhenComplete).unwrap(),
        *second_mesh
    );
    assert_eq!(
        rot_assets::decode_png(&sources[texture_id]).unwrap(),
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
            fs::read(root.path().join(".rot-assets.json")).unwrap(),
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
        assert!(!root.path().join(".rot-cache").exists());
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
    use rot_assets::{AssetMutation, AssetOutcome};
    use sha2::{Digest, Sha256};

    let fixture = PathBuf::from(std::env::var_os("ROT_TEST_GLB_FIXTURES").unwrap());
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("beacon.glb");
    fs::copy(fixture.join("beacon-A.glb"), &source).unwrap();
    let mut store = AssetStore::open(root.path()).unwrap();
    let first = store.import_glb_at(0, "beacon", "beacon.glb").unwrap();
    let old_meshes = store.mesh_snapshots();
    let old_textures = store.texture_snapshots();
    let catalog = root.path().join(".rot-assets.json");
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
        .join(".rot-models")
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
    assert!(!root.path().join(".rot-cache").exists());
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

fn model_bytes(nodes: Vec<serde_json::Value>, roughness: f32) -> Vec<u8> {
    model_with_image(nodes, roughness, None)
}

fn model_with_image(nodes: Vec<serde_json::Value>, roughness: f32, image: Option<&str>) -> Vec<u8> {
    model_with_sampler(nodes, roughness, image, None)
}

fn model_with_sampler(
    nodes: Vec<serde_json::Value>,
    roughness: f32,
    image: Option<&str>,
    sampler: Option<serde_json::Value>,
) -> Vec<u8> {
    use serde_json::json;
    let mut bin = [0.0_f32, 0., 0., 1., 0., 0., 0., 1., 0.]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    let mut document = json!({
        "asset":{"version":"2.0"},
        "scenes":[{"nodes":[0]}], "nodes":nodes,
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],
        "materials":[{"name":"Paint","pbrMetallicRoughness":{"metallicFactor":0,"roughnessFactor":roughness}}],
        "buffers":[{"byteLength":bin.len()}],
        "bufferViews":[{"buffer":0,"byteLength":bin.len()}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],
    });
    if let Some(uri) = image {
        document["extensionsUsed"] = json!(["KHR_texture_transform"]);
        document["images"] = json!([{"uri":uri}]);
        document["textures"] = json!([{"source":0}]);
        document["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"] =
            json!({"index":0,"extensions":{"KHR_texture_transform":{"texCoord":0}}});
        document["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"] = json!(1);
        document["accessors"]
            .as_array_mut()
            .unwrap()
            .push(json!({"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}));
        document["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(json!({"buffer":0,"byteOffset":bin.len(),"byteLength":24}));
        bin.extend(
            [0., 0., 1., 0., 0., 1.]
                .into_iter()
                .flat_map(f32::to_le_bytes),
        );
        document["meshes"][0]["primitives"][0]["indices"] = json!(2);
        document["accessors"]
            .as_array_mut()
            .unwrap()
            .push(json!({"bufferView":2,"componentType":5123,"count":6,"type":"SCALAR"}));
        document["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(json!({"buffer":0,"byteOffset":bin.len(),"byteLength":12}));
        bin.extend(
            [0_u16, 1, 2, 0, 0, 2]
                .into_iter()
                .flat_map(u16::to_le_bytes),
        );
        document["buffers"][0]["byteLength"] = json!(bin.len());
    }
    if let Some(sampler) = sampler {
        document["samplers"] = json!([sampler]);
        document["textures"][0]["sampler"] = json!(0);
    }
    let mut document = serde_json::to_vec(&document).unwrap();
    document.resize(document.len().next_multiple_of(4), b' ');
    let total = 28 + document.len() + bin.len();
    let mut bytes = b"glTF".to_vec();
    bytes.extend(2_u32.to_le_bytes());
    bytes.extend((total as u32).to_le_bytes());
    bytes.extend((document.len() as u32).to_le_bytes());
    bytes.extend(b"JSON");
    bytes.extend(document);
    bytes.extend((bin.len() as u32).to_le_bytes());
    bytes.extend(b"BIN\0");
    bytes.extend(bin);
    bytes
}

#[test]
fn optional_and_repeated_names_keep_distinct_paths_across_refresh_and_reopen() {
    use rot_assets::ModelInfo;
    use serde_json::json;

    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("assembly.glb");
    let nodes = vec![
        json!({"name":"Assembly","children":[1,2,3,4,5,6,7]}),
        json!({"name":"Twin","mesh":0}),
        json!({"name":"Twin","mesh":0,"translation":[2,0,0]}),
        json!({"name":"","mesh":0}),
        json!({"mesh":0}),
        json!({"name":"node-3","mesh":0,"translation":null,"rotation":null,"scale":null}),
        json!({"name":" ","mesh":0}),
        json!({"name":"Part/Subpart","mesh":0}),
    ];
    fs::write(&source, model_bytes(nodes.clone(), 0.6)).unwrap();
    let mut store = AssetStore::open(root.path()).unwrap();
    let first = store.import_glb_at(0, "assembly", "assembly.glb").unwrap();
    let names = [
        ("Assembly/ node-1", "Twin"),
        ("Assembly/ node-2", "Twin"),
        ("Assembly/ node-3", "Node 4"),
        ("Assembly/ node-4", "Node 5"),
        ("Assembly/node-3", "node-3"),
        ("Assembly/ node-6", "Node 7"),
        ("Assembly/ node-7", "Part/Subpart"),
    ];
    let mut resources = BTreeSet::new();
    for (path, name) in names {
        let node = &first.nodes[path];
        assert_eq!(node.name, name);
        assert_eq!(node.parent.as_deref(), Some("Assembly"));
        assert!(!ModelInfo::is_primitive_path(path));
        assert!(resources.insert(node.mesh_id.as_ref().unwrap()));
    }
    let named = &first.nodes["Assembly/node-3"];
    assert_eq!(named.translation, [0.; 3]);
    assert_eq!(named.rotation, [0.; 3]);
    assert_eq!(named.scale, [1.; 3]);

    let mut reordered = nodes;
    reordered[0]["children"] = json!([7, 6, 5, 4, 3, 2, 1]);
    fs::write(&source, model_bytes(reordered, 0.6)).unwrap();
    let refreshed = store.refresh_glb_at(1, "assembly").unwrap();
    assert_eq!(first.nodes, refreshed.nodes);
    assert_eq!(
        AssetStore::open(root.path()).unwrap().model("assembly"),
        Some(&refreshed)
    );
}

#[test]
fn asymmetric_matrix_preserves_column_major_pose_and_invalid_refresh_keeps_it() {
    use glam::{DMat4, DQuat, DVec3, EulerRot};
    use serde_json::json;

    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("matrix.glb");
    // +90 degrees around Y, distinct local X/Y/Z scales, and translation.
    let matrix = [
        0., 0., -2., 0., 0., 3., 0., 0., 4., 0., 0., 0., 7., -2., 5., 1.,
    ];
    let nodes = |mesh| vec![json!({"name":"Assembly","children":[1]}), mesh];
    fs::write(
        &source,
        model_bytes(
            nodes(json!({"name":"Asymmetric","mesh":0,"matrix":matrix})),
            0.6,
        ),
    )
    .unwrap();
    let mut store = AssetStore::open(root.path()).unwrap();
    let accepted = store.import_glb_at(0, "matrix", "matrix.glb").unwrap();
    let node = &accepted.nodes["Assembly/Asymmetric"];
    let [x, y, z] = node.rotation.map(f64::from);
    let rendered = DMat4::from_scale_rotation_translation(
        DVec3::from_array(node.scale.map(f64::from)),
        DQuat::from_euler(EulerRot::ZYX, z, y, x),
        DVec3::from_array(node.translation.map(f64::from)),
    );
    assert!(rendered
        .transform_point3(DVec3::new(1., 2., 3.))
        .abs_diff_eq(DVec3::new(19., 4., 3.), 1e-5));
    assert_eq!(node.scale, [2., 3., 4.]);

    let mut shear = matrix;
    shear[4] = 1.;
    let mut reflected = matrix;
    reflected[2] = 2.;
    let mut perspective = matrix;
    perspective[3] = 0.1;
    for (candidate, diagnostic) in [
        (json!({"matrix":[0,1]}), "16 finite column-major numbers"),
        (json!({"matrix":null}), "16 finite column-major numbers"),
        (
            json!({"matrix":matrix,"translation":[0,0,0]}),
            "either a matrix",
        ),
        (json!({"matrix":shear}), "contains shear"),
        (json!({"matrix":reflected}), "contains a reflection"),
        (json!({"matrix":perspective}), "must be affine"),
    ] {
        let mut mesh = candidate;
        mesh["name"] = json!("Asymmetric");
        mesh["mesh"] = json!(0);
        fs::write(&source, model_bytes(nodes(mesh), 0.6)).unwrap();
        let error = store.refresh_glb_at(1, "matrix").unwrap_err().to_string();
        assert!(
            error.contains("Assembly/Asymmetric") && error.contains(diagnostic),
            "{error}"
        );
        assert_eq!(store.revision(), 1);
        assert_eq!(store.model("matrix"), Some(&accepted));
    }
    assert_eq!(
        AssetStore::open(root.path()).unwrap().model("matrix"),
        Some(&accepted)
    );
}

#[test]
fn external_png_is_pinned_then_refreshed_by_a_texture_only_change() {
    use rot_assets::AssetMutation;
    use serde_json::json;
    let directory = tempfile::tempdir().unwrap();
    let assets = directory.path().join("Assets");
    let texture = assets.join("Textures/paint.png");
    fs::create_dir_all(texture.parent().unwrap()).unwrap();
    let png = |rgb: &[u8; 3]| {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(rgb)
            .unwrap();
        bytes
    };
    let first_png = png(&[200, 9, 32]);
    let second_png = png(&[7, 64, 190]);
    fs::write(&texture, &first_png).unwrap();
    let nodes = vec![json!({"name":"Mesh","mesh":0})];
    fs::write(
        assets.join("model.glb"),
        model_with_image(nodes.clone(), 0.6, Some("Textures/paint.png")),
    )
    .unwrap();
    fs::write(
        assets.join("outside.glb"),
        model_with_image(nodes, 0.6, Some("../outside.png")),
    )
    .unwrap();
    fs::write(directory.path().join("outside.png"), &first_png).unwrap();
    let (mut store, issues) = AssetStore::open_project(directory.path()).unwrap();
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(issues[0].contains("outside.glb") && issues[0].contains("parent/dot"));
    let first = store.models().next().unwrap().clone();
    assert_eq!(first.warnings, ["Skipped 1 degenerate triangle."]);
    let mesh = first.nodes["Mesh"].mesh_id.as_deref().unwrap();
    assert_eq!(store.mesh(mesh).unwrap().indices.len(), 3);
    let id = first.materials["Paint"].base_color_texture.clone().unwrap();
    let reads = |store: &AssetStore| {
        store
            .export_snapshot(&BTreeSet::from([id.clone()]))
            .unwrap()
            .read_sources()
            .unwrap()
            .remove(&id)
            .unwrap()
    };
    assert_eq!(reads(&store), first_png);

    fs::write(&texture, &second_png).unwrap();
    let reopened = AssetStore::open(directory.path()).unwrap();
    assert_eq!(reads(&reopened), first_png);
    let prepared = store
        .request_at(
            store.revision(),
            AssetMutation::RefreshGlb {
                id: first.id.clone(),
            },
        )
        .unwrap()
        .prepare();
    fs::write(&texture, &first_png).unwrap();
    let completion = store.adopt(prepared.publish()).unwrap();
    let failure = completion.result.unwrap_err();
    assert!(failure.to_string().contains("changed during preparation"));
    assert_eq!(reads(&store), first_png);
    fs::write(&texture, &second_png).unwrap();
    store.update_project_files(
        &BTreeSet::from(["Assets/Textures/paint.png".into()]),
        || true,
    );
    let second = store.model(&first.id).unwrap();
    assert_ne!(first.hash, second.hash);
    assert_eq!(first.materials, second.materials);
    assert_eq!(reads(&store), second_png);

    fs::remove_file(texture).unwrap();
    let reopened = AssetStore::open(directory.path()).unwrap();
    assert_eq!(reopened.model(&first.id), Some(second));
    assert_eq!(reads(&reopened), second_png);
}

#[test]
fn sampler_approximation_warns_without_changing_texture_identity() {
    use serde_json::json;
    let project = tempfile::tempdir().unwrap();
    let assets = project.path().join("Assets");
    fs::create_dir(&assets).unwrap();
    let source = assets.join("sampler.glb");
    let texture = fs::File::create(assets.join("paint.png")).unwrap();
    let mut png = png::Encoder::new(texture, 1, 1);
    png.set_color(png::ColorType::Rgb);
    png.set_depth(png::BitDepth::Eight);
    png.write_header()
        .unwrap()
        .write_image_data(&[120, 75, 20])
        .unwrap();
    let write = |mag, min| {
        fs::write(
            &source,
            model_with_sampler(
                vec![json!({"name":"Mesh","mesh":0})],
                0.6,
                Some("paint.png"),
                Some(json!({"magFilter":mag, "minFilter":min, "wrapS":33071, "wrapT":33071})),
            ),
        )
        .unwrap();
    };

    write(9728, 9984);
    let (mut store, issues) = AssetStore::open_project(project.path()).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    let original = store.models().next().unwrap().clone();
    let material = &original.materials["Paint"];
    assert!(!material.linear && !material.repeat);
    assert_eq!(original.warnings, ["Skipped 1 degenerate triangle."]);
    let texture = material.base_color_texture.as_ref().unwrap();
    let before = store.texture(texture).unwrap().clone();

    write(9728, 9985);
    let accepted = store
        .refresh_glb_at(store.revision(), &original.id)
        .unwrap();
    assert_eq!(accepted.materials, original.materials);
    assert_eq!(accepted.nodes, original.nodes);
    assert!(accepted.warnings.iter().any(|warning| {
        warning == "Paint: sampler mag NEAREST / min LINEAR_MIPMAP_NEAREST uses Rot Nearest with mipmaps"
    }));

    let revision = store.revision();
    write(1, 9985);
    let refused = store.refresh_glb_at(revision, &original.id).unwrap_err();
    assert!(refused.to_string().contains("magFilter 1"), "{refused}");
    assert_eq!(store.revision(), revision);
    let reopened = AssetStore::open(project.path()).unwrap();
    assert_eq!(reopened.model(&original.id), Some(&accepted));
    assert_eq!(reopened.texture(texture), Some(&before));
}

#[test]
fn import_refuses_unplaceable_materials_and_transforms_before_publication() {
    use serde_json::json;
    let depth_chain = |mesh_parent: bool| {
        let mut nodes: Vec<_> = (0..63)
            .map(|i| json!({"name":format!("Group{i}"),"children":[i + 1]}))
            .collect();
        if mesh_parent {
            nodes.push(json!({"name":"Wrapper","mesh":0,"children":[64]}));
            nodes.push(json!({"name":"EmptyChild"}));
        } else {
            nodes.push(json!({"name":"Leaf","mesh":0,"scale":[1,2,3]}));
        }
        nodes
    };
    for (nodes, roughness, diagnostic) in [
        (
            vec![json!({"name":"Mesh","mesh":0})],
            -0.07,
            "invalid factors or alpha mode",
        ),
        (
            vec![
                json!({"name":"MeshWithChild","mesh":0,"scale":[1,2,1],"children":[1]}),
                json!({"name":"Child"}),
            ],
            0.6,
            "Groups require positive uniform scale",
        ),
        (
            vec![
                json!({"name":"Group","scale":[1,2,1],"children":[1]}),
                json!({"name":"Mesh","mesh":0}),
            ],
            0.6,
            "Groups require positive uniform scale",
        ),
        (
            depth_chain(true),
            0.6,
            "64 Group levels with the placement root",
        ),
        (
            vec![
                json!({"name":"Group","translation":[10000,0,0],"children":[1]}),
                json!({"name":"Mesh","mesh":0,"translation":[1,0,0]}),
            ],
            0.6,
            "resolved pose exceeds translation",
        ),
        (
            vec![
                json!({"name":"Group","scale":[1000,1000,1000],"children":[1]}),
                json!({"name":"Mesh","mesh":0,"scale":[1,2,1]}),
            ],
            0.6,
            "resolved pose exceeds translation",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("model.glb"),
            model_bytes(nodes, roughness),
        )
        .unwrap();
        let mut store = AssetStore::open(directory.path()).unwrap();
        let failure = store.import_glb_at(0, "model", "model.glb").unwrap_err();
        assert!(failure.to_string().contains(diagnostic), "{failure}");
        assert_eq!(store.revision(), 0);
        assert!(store.model("model").is_none());
        assert!(!directory.path().join(".rot-assets.json").exists());
        assert!(!directory.path().join(".rot-models").exists());
    }

    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("model.glb"),
        model_bytes(depth_chain(false), 0.08),
    )
    .unwrap();
    let mut store = AssetStore::open(directory.path()).unwrap();
    let accepted = store.import_glb_at(0, "model", "model.glb").unwrap();
    assert_eq!(accepted.nodes.values().last().unwrap().scale, [1., 2., 3.]);
    assert_eq!(accepted.materials["Paint"].roughness, 0.08);
}

const RED_JPEG: &[u8] = &[
    255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0, 255, 219, 0, 67, 0, 2,
    1, 1, 1, 1, 1, 2, 1, 1, 1, 2, 2, 2, 2, 2, 4, 3, 2, 2, 2, 2, 5, 4, 4, 3, 4, 6, 5, 6, 6, 6, 5, 6,
    6, 6, 7, 9, 8, 6, 7, 9, 7, 6, 6, 8, 11, 8, 9, 10, 10, 10, 10, 10, 6, 8, 11, 12, 11, 10, 12, 9,
    10, 10, 10, 255, 219, 0, 67, 1, 2, 2, 2, 2, 2, 2, 5, 3, 3, 5, 10, 7, 6, 7, 10, 10, 10, 10, 10,
    10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
    10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 255, 192,
    0, 17, 8, 0, 1, 0, 1, 3, 1, 34, 0, 2, 17, 1, 3, 17, 1, 255, 196, 0, 31, 0, 0, 1, 5, 1, 1, 1, 1,
    1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 255, 196, 0, 181, 16, 0, 2, 1,
    3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 125, 1, 2, 3, 0, 4, 17, 5, 18, 33, 49, 65, 6, 19, 81, 97,
    7, 34, 113, 20, 50, 129, 145, 161, 8, 35, 66, 177, 193, 21, 82, 209, 240, 36, 51, 98, 114, 130,
    9, 10, 22, 23, 24, 25, 26, 37, 38, 39, 40, 41, 42, 52, 53, 54, 55, 56, 57, 58, 67, 68, 69, 70,
    71, 72, 73, 74, 83, 84, 85, 86, 87, 88, 89, 90, 99, 100, 101, 102, 103, 104, 105, 106, 115,
    116, 117, 118, 119, 120, 121, 122, 131, 132, 133, 134, 135, 136, 137, 138, 146, 147, 148, 149,
    150, 151, 152, 153, 154, 162, 163, 164, 165, 166, 167, 168, 169, 170, 178, 179, 180, 181, 182,
    183, 184, 185, 186, 194, 195, 196, 197, 198, 199, 200, 201, 202, 210, 211, 212, 213, 214, 215,
    216, 217, 218, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 241, 242, 243, 244, 245, 246,
    247, 248, 249, 250, 255, 196, 0, 31, 1, 0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1,
    2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 255, 196, 0, 181, 17, 0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1,
    2, 119, 0, 1, 2, 3, 17, 4, 5, 33, 49, 6, 18, 65, 81, 7, 97, 113, 19, 34, 50, 129, 8, 20, 66,
    145, 161, 177, 193, 9, 35, 51, 82, 240, 21, 98, 114, 209, 10, 22, 36, 52, 225, 37, 241, 23, 24,
    25, 26, 38, 39, 40, 41, 42, 53, 54, 55, 56, 57, 58, 67, 68, 69, 70, 71, 72, 73, 74, 83, 84, 85,
    86, 87, 88, 89, 90, 99, 100, 101, 102, 103, 104, 105, 106, 115, 116, 117, 118, 119, 120, 121,
    122, 130, 131, 132, 133, 134, 135, 136, 137, 138, 146, 147, 148, 149, 150, 151, 152, 153, 154,
    162, 163, 164, 165, 166, 167, 168, 169, 170, 178, 179, 180, 181, 182, 183, 184, 185, 186, 194,
    195, 196, 197, 198, 199, 200, 201, 202, 210, 211, 212, 213, 214, 215, 216, 217, 218, 226, 227,
    228, 229, 230, 231, 232, 233, 234, 242, 243, 244, 245, 246, 247, 248, 249, 250, 255, 218, 0,
    12, 3, 1, 0, 2, 17, 3, 17, 0, 63, 0, 249, 94, 138, 40, 175, 143, 63, 212, 67, 255, 217,
];

const BLUE_JPEG: &[u8] = &[
    255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0, 255, 219, 0, 67, 0, 2,
    1, 1, 1, 1, 1, 2, 1, 1, 1, 2, 2, 2, 2, 2, 4, 3, 2, 2, 2, 2, 5, 4, 4, 3, 4, 6, 5, 6, 6, 6, 5, 6,
    6, 6, 7, 9, 8, 6, 7, 9, 7, 6, 6, 8, 11, 8, 9, 10, 10, 10, 10, 10, 6, 8, 11, 12, 11, 10, 12, 9,
    10, 10, 10, 255, 219, 0, 67, 1, 2, 2, 2, 2, 2, 2, 5, 3, 3, 5, 10, 7, 6, 7, 10, 10, 10, 10, 10,
    10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
    10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 255, 192,
    0, 17, 8, 0, 1, 0, 1, 3, 1, 34, 0, 2, 17, 1, 3, 17, 1, 255, 196, 0, 31, 0, 0, 1, 5, 1, 1, 1, 1,
    1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 255, 196, 0, 181, 16, 0, 2, 1,
    3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 125, 1, 2, 3, 0, 4, 17, 5, 18, 33, 49, 65, 6, 19, 81, 97,
    7, 34, 113, 20, 50, 129, 145, 161, 8, 35, 66, 177, 193, 21, 82, 209, 240, 36, 51, 98, 114, 130,
    9, 10, 22, 23, 24, 25, 26, 37, 38, 39, 40, 41, 42, 52, 53, 54, 55, 56, 57, 58, 67, 68, 69, 70,
    71, 72, 73, 74, 83, 84, 85, 86, 87, 88, 89, 90, 99, 100, 101, 102, 103, 104, 105, 106, 115,
    116, 117, 118, 119, 120, 121, 122, 131, 132, 133, 134, 135, 136, 137, 138, 146, 147, 148, 149,
    150, 151, 152, 153, 154, 162, 163, 164, 165, 166, 167, 168, 169, 170, 178, 179, 180, 181, 182,
    183, 184, 185, 186, 194, 195, 196, 197, 198, 199, 200, 201, 202, 210, 211, 212, 213, 214, 215,
    216, 217, 218, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 241, 242, 243, 244, 245, 246,
    247, 248, 249, 250, 255, 196, 0, 31, 1, 0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1,
    2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 255, 196, 0, 181, 17, 0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1,
    2, 119, 0, 1, 2, 3, 17, 4, 5, 33, 49, 6, 18, 65, 81, 7, 97, 113, 19, 34, 50, 129, 8, 20, 66,
    145, 161, 177, 193, 9, 35, 51, 82, 240, 21, 98, 114, 209, 10, 22, 36, 52, 225, 37, 241, 23, 24,
    25, 26, 38, 39, 40, 41, 42, 53, 54, 55, 56, 57, 58, 67, 68, 69, 70, 71, 72, 73, 74, 83, 84, 85,
    86, 87, 88, 89, 90, 99, 100, 101, 102, 103, 104, 105, 106, 115, 116, 117, 118, 119, 120, 121,
    122, 130, 131, 132, 133, 134, 135, 136, 137, 138, 146, 147, 148, 149, 150, 151, 152, 153, 154,
    162, 163, 164, 165, 166, 167, 168, 169, 170, 178, 179, 180, 181, 182, 183, 184, 185, 186, 194,
    195, 196, 197, 198, 199, 200, 201, 202, 210, 211, 212, 213, 214, 215, 216, 217, 218, 226, 227,
    228, 229, 230, 231, 232, 233, 234, 242, 243, 244, 245, 246, 247, 248, 249, 250, 255, 218, 0,
    12, 3, 1, 0, 2, 17, 3, 17, 0, 63, 0, 248, 206, 138, 40, 175, 245, 16, 254, 83, 63, 255, 217,
];

fn encode_test_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(*chunk.get(1).unwrap_or(&0));
        let b2 = u32::from(*chunk.get(2).unwrap_or(&0));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[test]
fn gltf_with_external_bin_and_jpeg_texture_imports_reopens_and_refreshes() {
    use serde_json::json;
    let directory = tempfile::tempdir().unwrap();
    let assets = directory.path().join("Assets");
    let tex_dir = assets.join("Textures");
    fs::create_dir_all(&tex_dir).unwrap();

    let positions = [0.0_f32, 0., 0., 1., 0., 0., 0., 1., 0.];
    let uvs = [0.0_f32, 0., 1., 0., 0., 1.];
    let mut bin: Vec<u8> = positions.into_iter().flat_map(f32::to_le_bytes).collect();
    bin.extend(uvs.into_iter().flat_map(f32::to_le_bytes));
    let bin_len = bin.len();
    fs::write(assets.join("crate.bin"), &bin).unwrap();
    fs::write(tex_dir.join("albedo.jpg"), RED_JPEG).unwrap();

    let gltf = json!({
        "asset":{"version":"2.0","generator":"Blender 4.2"},
        "scene":0,
        "scenes":[{"nodes":[0]}],
        "nodes":[{"name":"Crate","mesh":0}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0,"TEXCOORD_0":1},"material":0}]}],
        "materials":[{"name":"Paint","pbrMetallicRoughness":{"baseColorTexture":{"index":0},"metallicFactor":0,"roughnessFactor":0.6}}],
        "textures":[{"source":0}],
        "images":[{"uri":"Textures/albedo.jpg"}],
        "buffers":[{"uri":"crate.bin","byteLength":bin_len}],
        "bufferViews":[
            {"buffer":0,"byteOffset":0,"byteLength":36},
            {"buffer":0,"byteOffset":36,"byteLength":24}
        ],
        "accessors":[
            {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},
            {"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}
        ]
    });
    fs::write(
        assets.join("crate.gltf"),
        serde_json::to_vec(&gltf).unwrap(),
    )
    .unwrap();

    // Path traversal from .gltf buffer URI must still refuse cleanly.
    let mut escaping = gltf.clone();
    escaping["buffers"][0]["uri"] = json!("../secret.bin");
    fs::write(directory.path().join("secret.bin"), &bin).unwrap();
    fs::write(
        assets.join("escaping.gltf"),
        serde_json::to_vec(&escaping).unwrap(),
    )
    .unwrap();

    let (mut store, issues) = AssetStore::open_project(directory.path()).unwrap();
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(issues[0].contains("escaping.gltf"));

    let first = store.models().next().unwrap().clone();
    let tex_id = first.materials["Paint"]
        .base_color_texture
        .as_ref()
        .unwrap()
        .clone();
    let initial_tex = store.texture(&tex_id).unwrap().clone();
    assert!(initial_tex.mips[0].bytes[0] > 170 && initial_tex.mips[0].bytes[2] < 90);
    let snapshot_png = store
        .export_snapshot(&BTreeSet::from([tex_id.clone()]))
        .unwrap()
        .read_sources()
        .unwrap()
        .remove(&tex_id)
        .unwrap();
    assert_eq!(rot_assets::decode_png(&snapshot_png).unwrap(), initial_tex);

    // Updating the external JPEG refreshes the accepted model hash.
    fs::write(tex_dir.join("albedo.jpg"), BLUE_JPEG).unwrap();
    store.update_project_files(
        &BTreeSet::from(["Assets/Textures/albedo.jpg".into()]),
        || true,
    );
    let refreshed = store.model(&first.id).unwrap().clone();
    assert_ne!(refreshed.hash, first.hash);
    let updated_tex = store.texture(&tex_id).unwrap().clone();
    assert!(updated_tex.mips[0].bytes[2] > 170 && updated_tex.mips[0].bytes[0] < 90);

    // Accepted model survives deleting source .bin/.jpg and also supports data: URIs.
    fs::remove_file(assets.join("crate.bin")).unwrap();
    fs::remove_file(tex_dir.join("albedo.jpg")).unwrap();
    let reopened = AssetStore::open(directory.path()).unwrap();
    assert_eq!(reopened.model(&first.id), Some(&refreshed));

    let mut data_uri_gltf = gltf;
    data_uri_gltf["buffers"][0]["uri"] = json!(format!(
        "data:application/octet-stream;base64,{}",
        encode_test_base64(&bin)
    ));
    data_uri_gltf["images"][0]["uri"] = json!(format!(
        "data:image/jpeg;base64,{}",
        encode_test_base64(RED_JPEG)
    ));
    let embedded_path = assets.join("embedded.gltf");
    fs::write(&embedded_path, serde_json::to_vec(&data_uri_gltf).unwrap()).unwrap();
    let mut store2 = AssetStore::open(directory.path()).unwrap();
    let embedded = store2
        .import_glb_at(store2.revision(), "embedded", "Assets/embedded.gltf")
        .unwrap();
    let emb_tex = embedded.materials["Paint"]
        .base_color_texture
        .as_ref()
        .unwrap();
    assert!(store2.texture(emb_tex).unwrap().mips[0].bytes[0] > 170);
}

#[test]
fn blender_and_cc0_real_world_gltf_features_import_with_warnings() {
    use serde_json::json;
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("prop.glb");

    // 3 positions + 3 non-unit normals + 2 lines + 3 colors + JPEG + WebP + CUBICSPLINE (3*2 VEC4 + 2 times)
    let mut bin: Vec<u8> = [
        0.0_f32, 0., 0., 1., 0., 0., 0., 1., 0., // 0..36: positions
        0., 0., 1.05, 0., 0., 1.05, 0., 0., 1.05, // 36..72: unnormalized normals
        1., 0., 0., 1., 0., 1., 0., 1., 0., 0., 1., 1., // 72..120: COLOR_0 VEC4
        0., 1., // 120..128: times
        // 128..224: CUBICSPLINE 2 keyframes * 3 VEC4 (in, val, out)
        0., 0., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.70710677,
        0.70710677, 0., 0., 0., 0.,
    ]
    .into_iter()
    .flat_map(f32::to_le_bytes)
    .collect();
    let jpg_offset = bin.len();
    bin.extend(RED_JPEG);
    let webp_offset = bin.len();
    bin.extend(b"RIFF\x00\x00\x00\x00WEBP");

    let doc = json!({
        "asset":{"version":"2.0","generator":"Blender 4.2","extras":{"exporter":"khronos"}},
        "extensionsUsed":["KHR_materials_ior","KHR_materials_emissive_strength"],
        "scene":1,
        "scenes":[{"nodes":[1]},{"nodes":[0],"extras":{"collection":"Props"}}],
        "skins":[{"joints":[0]}],
        "nodes":[
            {"name":"Prop","mesh":0,"skin":0,"extras":{"custom_prop":42}},
            {"name":"IgnoredSceneNode"}
        ],
        "meshes":[{
            "name":"PropMesh",
            "extras":{"smooth":true},
            "primitives":[
                {
                    "attributes":{"POSITION":0,"NORMAL":1,"COLOR_0":2},
                    "material":0,
                    "targets":[{"POSITION":0}],
                    "extras":{"slot":0}
                },
                {
                    "attributes":{"POSITION":0},
                    "mode":1
                },
                {
                    "attributes":{"POSITION":0},
                    "material":1
                }
            ]
        }],
        "materials":[
            {
                "name":"Wood",
                "extensions":{"KHR_materials_ior":{"ior":1.45}},
                "pbrMetallicRoughness":{
                    "baseColorTexture":{"index":0},
                    "metallicFactor":0,
                    "roughnessFactor":0.5
                }
            },
            {
                "name":"Wood",
                "pbrMetallicRoughness":{
                    "metallicRoughnessTexture":{"index":1},
                    "metallicFactor":0,
                    "roughnessFactor":0.5
                }
            }
        ],
        "samplers":[{"wrapS":33648,"wrapT":33648}],
        "images":[
            {"bufferView":4,"mimeType":"image/jpeg"},
            {"bufferView":5,"mimeType":"image/webp"}
        ],
        "textures":[{"source":0,"sampler":0},{"source":1}],
        "buffers":[{"byteLength":bin.len()}],
        "bufferViews":[
            {"buffer":0,"byteOffset":0,"byteLength":36},
            {"buffer":0,"byteOffset":36,"byteLength":36},
            {"buffer":0,"byteOffset":72,"byteLength":48},
            {"buffer":0,"byteOffset":120,"byteLength":104},
            {"buffer":0,"byteOffset":jpg_offset,"byteLength":RED_JPEG.len()},
            {"buffer":0,"byteOffset":webp_offset,"byteLength":12}
        ],
        "accessors":[
            {"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"},
            {"bufferView":1,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"},
            {"bufferView":2,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC4"},
            {"bufferView":3,"byteOffset":0,"componentType":5126,"count":2,"type":"SCALAR"},
            {"bufferView":3,"byteOffset":8,"componentType":5126,"count":6,"type":"VEC4"}
        ],
        "animations":[{
            "channels":[
                {"sampler":0,"target":{"node":0,"path":"rotation"}},
                {"sampler":0,"target":{"node":0,"path":"weights"}}
            ],
            "samplers":[{"input":3,"output":4,"interpolation":"CUBICSPLINE"}]
        }]
    });

    // Pack GLB inline using JSON + BIN
    let mut json_bytes = serde_json::to_vec(&doc).unwrap();
    json_bytes.resize(json_bytes.len().next_multiple_of(4), b' ');
    bin.resize(bin.len().next_multiple_of(4), 0);
    let total = 28 + json_bytes.len() + bin.len();
    let mut glb = b"glTF".to_vec();
    glb.extend(2_u32.to_le_bytes());
    glb.extend((total as u32).to_le_bytes());
    glb.extend((json_bytes.len() as u32).to_le_bytes());
    glb.extend(b"JSON");
    glb.extend(json_bytes);
    glb.extend((bin.len() as u32).to_le_bytes());
    glb.extend(b"BIN\0");
    glb.extend(bin);
    fs::write(&source, glb).unwrap();

    let mut store = AssetStore::open(root.path()).unwrap();
    let model = store.import_glb_at(0, "prop", "prop.glb").unwrap();
    assert!(model.materials.contains_key("Wood"));
    assert!(model.materials.contains_key("Wood (2)"));
    assert!(model.materials["Wood"].base_color_texture.is_some());
    assert!(model.materials["Wood (2)"].roughness_texture.is_none());
    assert!(model.clips.contains_key("Clip 1"));

    for snippet in [
        "additional scene(s) ignored",
        "Ignored optional glTF extension(s): KHR_materials_emissive_strength, KHR_materials_ior",
        "Skeletal skinning omitted",
        "Morph targets omitted",
        "Vertex colors omitted",
        "Generated default UVs",
        "Skipped 1 non-triangle primitive(s)",
        "sampler wrap mode approximated as Repeat",
        "duplicate material name disambiguated",
        "roughness map omitted (unsupported image format)",
        "cubic spline interpolation imported as linear keys",
        "morph target weights omitted",
    ] {
        assert!(
            model
                .warnings
                .iter()
                .any(|warning| warning.contains(snippet)),
            "missing warning {snippet:?} in {:?}",
            model.warnings
        );
    }

    let reopened = AssetStore::open(root.path()).unwrap();
    assert_eq!(reopened.model("prop"), Some(&model));
}
