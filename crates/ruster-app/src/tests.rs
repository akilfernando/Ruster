use super::*;

struct Counter {
    content: Content,
    count: u64,
    goal: u64,
    fail: bool,
    dt: Duration,
}
impl Counter {
    fn new(goal: u64) -> Self {
        Self {
            content: Content::new(Scene::demo(), MeshMap::new()).unwrap(),
            count: 0,
            goal,
            fail: false,
            dt: Duration::from_millis(10),
        }
    }
}
impl Application for Counter {
    fn content(&self) -> &Content {
        &self.content
    }
    fn tick(&mut self, step: Step) -> Result<(), String> {
        assert_eq!(step.tick, self.count + 1);
        assert_eq!(step.dt, self.dt);
        if self.fail {
            return Err("behavior refused tick".into());
        }
        self.count += 1;
        self.content.scene_mut().objects[0].transform.translation[0] = self.count as f32;
        Ok(())
    }
    fn reset(&mut self) -> Result<(), String> {
        self.count = 0;
        self.fail = false;
        self.content = Content::new(Scene::demo(), MeshMap::new())?;
        Ok(())
    }
    fn completed(&self) -> bool {
        self.count >= self.goal
    }
}

#[test]
fn explicit_replay_reset_and_normal_completion_share_one_lifecycle() {
    let mut runtime = Runtime::new(Counter::new(3), Duration::from_millis(10)).unwrap();
    for _ in 0..3 {
        runtime.step().unwrap();
    }
    let expected = runtime.app().content().scene().fingerprint();
    assert_eq!(runtime.tick(), 3);
    runtime.step().unwrap(); // A completed application never receives an extra step.
    assert_eq!(runtime.tick(), 3);
    runtime.reset().unwrap();
    assert!(!runtime.is_running());
    assert_eq!(runtime.tick(), 0);
    runtime.set_running(true);
    assert_eq!(runtime.advance(Duration::from_millis(100)).unwrap(), 3);
    assert!(!runtime.is_running());
    assert_eq!(runtime.app().content().scene().fingerprint(), expected);
}

#[test]
fn scheduling_drops_stall_debt_and_pause_does_not_leak_elapsed_time() {
    let mut runtime = Runtime::new(Counter::new(100), Duration::from_millis(10)).unwrap();
    runtime.set_running(true);
    assert_eq!(runtime.advance(Duration::from_secs(3600)).unwrap(), 4);
    assert_eq!(runtime.advance(Duration::ZERO).unwrap(), 0);
    runtime.set_running(false);
    assert_eq!(runtime.advance(Duration::from_secs(3600)).unwrap(), 0);
    runtime.set_running(true);
    assert_eq!(runtime.advance(Duration::from_millis(9)).unwrap(), 0);
    assert_eq!(runtime.advance(Duration::from_millis(1)).unwrap(), 1);
    assert_eq!(runtime.tick(), 5);
    for dt in [Duration::from_millis(500), Duration::from_secs(1)] {
        let mut counter = Counter::new(100);
        counter.dt = dt;
        let mut runtime = Runtime::new(counter, dt).unwrap();
        runtime.set_running(true);
        assert_eq!(runtime.advance(dt).unwrap(), 1);
        assert_eq!(runtime.tick(), 1);
    }
}

#[test]
fn behavior_or_invalid_scene_stops_ticks_and_reset_recovers() {
    let mut runtime = Runtime::new(Counter::new(100), Duration::from_millis(10)).unwrap();
    runtime.app_mut().fail = true;
    runtime.set_running(true);
    assert!(runtime
        .advance(Duration::from_millis(10))
        .unwrap_err()
        .contains("refused"));
    assert!(!runtime.is_running());
    assert_eq!(runtime.tick(), 0);
    runtime.reset().unwrap();
    runtime.app_mut().content.scene_mut().objects[0].color[0] = f32::NAN;
    runtime.set_running(true);
    assert!(runtime.step().unwrap_err().contains("color"));
    assert!(!runtime.is_running());
    assert_eq!(runtime.tick(), 0);
    runtime.reset().unwrap();
    runtime.step().unwrap();
    assert_eq!(runtime.tick(), 1);
}

#[test]
fn content_rejects_missing_references_and_invalid_mesh_before_rendering() {
    let mesh = ruster_assets::parse_obj(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let mut scene = Scene::demo();
    scene.objects[0].mesh = Some("triangle".into());
    assert!(Content::new(scene.clone(), MeshMap::new())
        .err()
        .unwrap()
        .contains("missing mesh triangle"));
    let mut invalid = mesh.clone();
    invalid.indices[0] = 100;
    assert!(Content::new(
        scene.clone(),
        [("triangle".into(), Arc::new(invalid))].into()
    )
    .is_err());
    let mut invalid = mesh.clone();
    invalid.vertices[0].normal = [0.0; 3];
    assert!(Content::new(
        scene.clone(),
        [("triangle".into(), Arc::new(invalid))].into()
    )
    .is_err());
    assert!(Content::new(
        scene.clone(),
        [("triangle".into(), Arc::new(mesh.clone()))].into()
    )
    .is_ok());
    scene.materials.insert(
        "shared".into(),
        ruster_scene::OpaqueMaterial {
            base_color_texture: Some("color".into()),
            ..Default::default()
        },
    );
    scene.objects[0].material = Some("shared".into());
    let meshes = MeshMap::from([("triangle".into(), Arc::new(mesh.clone()))]);
    assert!(Content::new(scene.clone(), meshes.clone())
        .err()
        .unwrap()
        .contains("missing texture color"));
    let texture = Arc::new(ruster_assets::Texture {
        role: ruster_assets::TextureRole::BaseColor,
        width: 1,
        height: 1,
        mips: vec![ruster_assets::TextureMip {
            width: 1,
            height: 1,
            bytes: vec![255; 4],
        }],
    });
    let textures = [("color".into(), texture.clone())].into();
    assert!(Content::with_textures(scene.clone(), meshes, textures)
        .err()
        .unwrap()
        .contains("complete UVs"));
    let mut uv_mesh = mesh;
    uv_mesh.texcoords = Some(vec![[0.0; 2]; uv_mesh.vertices.len()]);
    let mut content = Content::with_textures(
        scene,
        [("triangle".into(), Arc::new(uv_mesh))].into(),
        [("color".into(), texture.clone())].into(),
    )
    .unwrap();
    assert!(Arc::ptr_eq(&texture, &content.textures()["color"]));
    // Even an inactive Diffuse/Legacy map is a faithful launch dependency.
    content
        .scene_mut()
        .materials
        .get_mut("shared")
        .unwrap()
        .roughness_texture = Some("color".into());
    assert!(content.validate().unwrap_err().contains("wrong role"));
    content
        .scene_mut()
        .materials
        .get_mut("shared")
        .unwrap()
        .roughness_texture = Some("roughness".into());
    assert!(content
        .validate()
        .unwrap_err()
        .contains("missing texture roughness"));
    let roughness = Arc::new(ruster_assets::Texture {
        role: ruster_assets::TextureRole::Roughness,
        width: 1,
        height: 1,
        mips: vec![ruster_assets::TextureMip {
            width: 1,
            height: 1,
            bytes: vec![128],
        }],
    });
    let mut textures = content.textures().clone();
    textures.insert("roughness".into(), roughness.clone());
    content = Content::with_textures(content.scene().clone(), content.meshes().clone(), textures)
        .unwrap();
    assert!(Arc::ptr_eq(&roughness, &content.textures()["roughness"]));
    content
        .scene_mut()
        .materials
        .get_mut("shared")
        .unwrap()
        .base_color_texture = Some("unavailable".into());
    assert!(content
        .validate()
        .unwrap_err()
        .contains("missing texture unavailable"));

    let mip_chain = |role: ruster_assets::TextureRole| {
        let mut edge = 2048;
        let mut mips = Vec::new();
        loop {
            mips.push(ruster_assets::TextureMip {
                width: edge,
                height: edge,
                bytes: vec![255; edge as usize * edge as usize * role.bytes_per_pixel()],
            });
            if edge == 1 {
                break;
            }
            edge /= 2;
        }
        Arc::new(ruster_assets::Texture {
            role,
            width: 2048,
            height: 2048,
            mips,
        })
    };
    let color_chain = mip_chain(ruster_assets::TextureRole::BaseColor);
    let data_chain = mip_chain(ruster_assets::TextureRole::Roughness);
    let plain_scene = Scene::demo();
    // Shared immutable allocations still count once per mapped asset ID.
    let near: TextureMap = (0..3)
        .map(|index| (format!("color-{index}"), color_chain.clone()))
        .collect();
    assert_eq!(
        near.values()
            .map(|value| value.payload_bytes())
            .sum::<usize>(),
        ruster_assets::MAX_TEXTURE_RESOLVED_BYTES - 4
    );
    Content::with_textures(plain_scene.clone(), MeshMap::new(), near.clone()).unwrap();
    let mut exact = near;
    exact.insert("tiny".into(), texture.clone());
    Content::with_textures(plain_scene.clone(), MeshMap::new(), exact.clone()).unwrap();
    let mut over = exact;
    over.insert("overflow".into(), roughness.clone());
    assert!(
        Content::with_textures(plain_scene.clone(), MeshMap::new(), over.clone())
            .err()
            .unwrap()
            .contains("64 MiB")
    );
    let mixed: TextureMap = (0..2)
        .map(|index| (format!("color-{index}"), color_chain.clone()))
        .chain((0..4).map(|index| (format!("data-{index}"), data_chain.clone())))
        .collect();
    assert_eq!(
        mixed
            .values()
            .map(|value| value.payload_bytes())
            .sum::<usize>(),
        ruster_assets::MAX_TEXTURE_RESOLVED_BYTES - 4
    );
    Content::with_textures(plain_scene.clone(), MeshMap::new(), mixed).unwrap();
    let invalid_texture = Arc::new(ruster_assets::Texture {
        role: ruster_assets::TextureRole::BaseColor,
        width: 1,
        height: 1,
        mips: vec![ruster_assets::TextureMip {
            width: 1,
            height: 1,
            bytes: vec![255; 3],
        }],
    });
    over.insert("zz-invalid".into(), invalid_texture.clone());
    assert!(
        Content::with_textures(plain_scene.clone(), MeshMap::new(), over)
            .err()
            .unwrap()
            .contains("texture zz-invalid:")
    );

    let triangle = content.meshes()["triangle"].as_ref();
    let large_mesh = Arc::new(ruster_assets::Mesh {
        vertices: triangle
            .vertices
            .iter()
            .cloned()
            .cycle()
            .take(ruster_assets::MAX_VERTICES)
            .collect(),
        indices: triangle
            .indices
            .iter()
            .copied()
            .cycle()
            .take(ruster_assets::MAX_TRIANGLES * 3)
            .collect(),
        texcoords: Some(vec![[0.0; 2]; ruster_assets::MAX_VERTICES]),
    });
    let large_count = ruster_assets::MAX_RESOLVED_BYTES / large_mesh.payload_bytes();
    let remaining = ruster_assets::MAX_RESOLVED_BYTES - large_count * large_mesh.payload_bytes();
    let tail_vertex_count = (remaining - 24 * 4) / 32;
    let tail = Arc::new(ruster_assets::Mesh {
        // Eight triangles leave an exact multiple of32-byte UV-complete vertices.
        vertices: triangle
            .vertices
            .iter()
            .cloned()
            .cycle()
            .take(tail_vertex_count)
            .collect(),
        indices: [0, 1, 2].repeat(8),
        texcoords: Some(vec![[0.0; 2]; tail_vertex_count]),
    });
    let mut meshes: MeshMap = (0..large_count)
        .map(|index| (format!("large-{index:02}"), large_mesh.clone()))
        .collect();
    meshes.insert("tail".into(), tail);
    assert_eq!(
        meshes
            .values()
            .map(|value| value.payload_bytes())
            .sum::<usize>(),
        ruster_assets::MAX_RESOLVED_BYTES
    );
    Content::new(plain_scene.clone(), meshes.clone()).unwrap();
    meshes.insert("overflow".into(), content.meshes()["triangle"].clone());
    assert!(Content::new(plain_scene.clone(), meshes.clone())
        .err()
        .unwrap()
        .contains("48 MiB"));
    // Mesh validation/budget precede texture validation, while all geometry
    // validation precedes the mesh budget in the existing Content contract.
    assert!(Content::with_textures(
        plain_scene.clone(),
        meshes.clone(),
        [("bad".into(), invalid_texture)].into()
    )
    .err()
    .unwrap()
    .contains("48 MiB"));
    let mut invalid_mesh = triangle.clone();
    invalid_mesh.indices[0] = u32::MAX;
    meshes.insert("zz-invalid".into(), Arc::new(invalid_mesh));
    assert!(Content::new(plain_scene, meshes)
        .err()
        .unwrap()
        .contains("mesh zz-invalid:"));
}

#[test]
fn resource_root_loads_read_only_and_rejects_bad_or_escaping_resources() {
    let temporary = tempfile::tempdir().unwrap();
    let resource_dir = temporary.path().join("resources");
    std::fs::create_dir(&resource_dir).unwrap();
    Scene::demo().save(resource_dir.join("scene.json")).unwrap();
    std::fs::write(
        resource_dir.join("triangle.obj"),
        b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
    )
    .unwrap();
    std::fs::write(resource_dir.join("bad.obj"), b"broken").unwrap();
    let root = ResourceRoot::new(&resource_dir).unwrap();
    assert_eq!(root.scene("scene.json").unwrap(), Scene::demo());
    assert_eq!(root.mesh("triangle.obj").unwrap().indices.len(), 3);
    assert!(root.mesh("bad.obj").unwrap_err().contains("bad.obj"));
    assert!(root
        .scene("missing.json")
        .unwrap_err()
        .contains("missing.json"));
    assert!(root
        .scene("triangle.obj")
        .unwrap_err()
        .contains("scene resource"));
    assert!(root.scene("subdirectory").is_err());
    for path in ["", "../scene.json", "/etc/passwd"] {
        assert!(root.resolve(path).is_err());
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(temporary.path(), resource_dir.join("outside")).unwrap();
        assert!(root.resolve("outside").unwrap_err().contains("escapes"));
        let _socket = std::os::unix::net::UnixListener::bind(resource_dir.join("socket")).unwrap();
        assert!(root.scene("socket").unwrap_err().contains("regular file"));
        assert!(root.mesh("socket").unwrap_err().contains("regular file"));
    }
    // Neither loader creates an asset catalog or a cache in the resource directory.
    assert!(!resource_dir.join(".ruster-cache").exists());
    assert!(!resource_dir.join(".ruster-assets.json").exists());
}
