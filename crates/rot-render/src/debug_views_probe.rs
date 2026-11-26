use super::*;
use crate::lighting_gpu::PackedLighting;
use crate::material_gpu::BindingKey;
use glam::{Mat3, Mat4, Quat, Vec3};
use rot_scene::{
    AlphaMode, Ambient, AntiAliasing, BackgroundColor, DisplayProfile, Light, LightKind, Lighting,
    PostProcess, SsaoSettings, SunShadow,
};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

fn setup_vk_device() -> (wgpu::Adapter, wgpu::Device, wgpu::Queue) {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    configure_setup(&mut setup);
    let instance = wgpu::Instance::new(setup.instance_descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .expect("Vulkan adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&(setup.device_descriptor)(&adapter)))
            .expect("Vulkan device");
    (adapter, device, queue)
}

fn instance(
    translation: Vec3,
    scale: Vec3,
    base_color: [f32; 3],
    roughness: f32,
    metallic: f32,
) -> Instance {
    let model = Mat4::from_scale_rotation_translation(scale, Quat::IDENTITY, translation);
    let inv_t = Mat3::from_mat4(model).inverse().transpose();
    Instance {
        model: model.to_cols_array_2d(),
        normal: [
            inv_t.x_axis.extend(0.0).to_array(),
            inv_t.y_axis.extend(0.0).to_array(),
            inv_t.z_axis.extend(0.0).to_array(),
        ],
        color: [1.0; 4],
        base_color: [base_color[0], base_color[1], base_color[2], 0.0],
        response: [1.0, roughness, 0.0, 0.0],
        emission_color: [0.0; 4],
        uv_transform: [1.0, 1.0, 0.0, 0.0],
        pbr_params: [metallic, 1.0, 1.0, 1.0],
        pbr_maps: [0.0, 0.0, 0.0, -1.0],
    }
}

fn cube_group(instances: Vec<Instance>) -> DrawGroup {
    let count = instances.len() as u32;
    DrawGroup {
        alpha_mode: AlphaMode::Opaque,
        double_sided: false,
        key: None,
        primitive: None,
        binding: BindingKey::default(),
        texture: None,
        roughness: None,
        metallic: None,
        normal: None,
        emissive: None,
        occlusion: None,
        mesh: None,
        range: 0..count,
        visible_range: 0..count,
        instances,
    }
}

fn sample_world(bytes: &[u8], [w, h]: [u32; 2], vp: Mat4, world: Vec3) -> [u8; 3] {
    let clip = vp * world.extend(1.0);
    let ndc = clip.truncate() / clip.w;
    assert!(
        ndc.x.abs() <= 0.99 && ndc.y.abs() <= 0.99 && ndc.z >= 0.0 && ndc.z <= 1.0,
        "point {world:?} outside frustum: {ndc:?}"
    );
    let px = ((ndc.x * 0.5 + 0.5) * w as f32).clamp(0.0, (w - 1) as f32) as usize;
    let py = ((0.5 - ndc.y * 0.5) * h as f32).clamp(0.0, (h - 1) as f32) as usize;
    let off = (py * w as usize + px) * 4;
    [bytes[off], bytes[off + 1], bytes[off + 2]]
}

#[test]
#[ignore = "requires GPU Vulkan adapter"]
fn shading_debug_views_gpu_probe() {
    let (_adapter, device, queue) = setup_vk_device();
    assert!(
        device
            .features()
            .contains(wgpu::Features::POLYGON_MODE_LINE),
        "Vulkan GPU probe expects POLYGON_MODE_LINE for hardware wireframe"
    );
    let mut r = Resources::new(&device, &queue, COLOR, 1);
    r.enable_hdr(&device, 1);
    let mut resources = egui_wgpu::CallbackResources::default();
    resources.insert(r);
    let context = egui::Context::default();
    let size = [320_u32, 240_u32];

    let render_mode = |resources: &mut egui_wgpu::CallbackResources,
                       mode: ShadingMode,
                       view: Mat4,
                       proj: Mat4,
                       eye: Vec3,
                       lighting: &PackedLighting,
                       shadow: Option<SunShadow>,
                       post: Option<PostProcess>,
                       groups: Vec<DrawGroup>| {
        context
            .run_pass(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
        let cb = SceneCallback {
            background: Some(BackgroundColor::new([0.0, 0.0, 0.0]).unwrap()),
            sky: None,
            fog: None,
            post_process: post,
            shading_mode: mode,
            camera: (proj * view).to_cols_array(),
            inv_camera: (proj * view).inverse().to_cols_array(),
            cluster_camera: ClusterCamera::from_matrices(view, proj),
            camera_view: view.to_cols_array(),
            camera_projection: proj.to_cols_array(),
            show_editor_grid: false,
            camera_eye_mode: eye.extend(1.0).to_array(),
            display: DisplayProfile::SurfaceHdr { exposure_stops: 0. },
            lighting: lighting.clone(),
            groups,
            size: egui::vec2(size[0] as f32, size[1] as f32),
            cpu_us: 0.,
            culled_objects: 0,
            shadow,
            context: context.clone(),
            receipt: FrameReceipt::default(),
            prepared: AtomicBool::new(false),
        };
        selection_probe::render_sized(&device, &queue, resources, &cb, COLOR, size)
    };

    // --- 1. Shared PBR / Geometry / Shadow scene (Lit, Unlit, Wireframe, Normals, Roughness, Metallic, Albedo, Depth) ---
    let mut lights = BTreeMap::new();
    let mut sun = Light::directional(None);
    sun.intensity = 2.5;
    sun.transform.rotation = [-std::f32::consts::FRAC_PI_4, 0.35, 0.0];
    lights.insert("sun".into(), sun);
    let sun_shadow = Some(SunShadow {
        enabled: true,
        ..Default::default()
    });
    let packed_sun = PackedLighting::try_pack(&Lighting::Authored {
        ambient: Ambient {
            color: [0.02; 3],
            intensity: 0.05,
        },
        lights,
    })
    .unwrap();

    let eye = Vec3::new(0.0, 1.4, 6.5);
    let view = Mat4::look_at_rh(eye, Vec3::new(0.0, 0.2, 0.0), Vec3::Y);
    let proj = Mat4::perspective_rh(50.0_f32.to_radians(), 320.0 / 240.0, 0.5, 60.0);
    let vp = proj * view;

    let groups = vec![cube_group(vec![
        // Floor receiving shadow from the overhead awning
        instance(
            Vec3::new(0.0, -0.5, -2.0),
            Vec3::new(14.0, 0.4, 14.0),
            [0.85, 0.85, 0.85],
            0.7,
            0.0,
        ),
        // Overhead awning casting sun shadow onto z in [0.3..1.7] while leaving camera view under y=1.0 clear
        instance(
            Vec3::new(0.0, 1.4, 2.5),
            Vec3::new(2.0, 0.8, 1.0),
            [0.85, 0.85, 0.85],
            0.7,
            0.0,
        ),
        // Glossy metallic red panel (front-left)
        instance(
            Vec3::new(-2.5, 0.8, 1.5),
            Vec3::new(1.4, 1.2, 0.4),
            [0.90, 0.10, 0.10],
            0.15,
            0.95,
        ),
        // Matte dielectric green panel (front-right)
        instance(
            Vec3::new(2.5, 0.8, 1.5),
            Vec3::new(1.4, 1.2, 0.4),
            [0.10, 0.90, 0.10],
            0.85,
            0.05,
        ),
        // Far neutral panel for depth comparison (above awning line of sight)
        instance(
            Vec3::new(0.0, 3.0, -14.0),
            Vec3::new(3.0, 2.0, 0.5),
            [0.8, 0.8, 0.8],
            0.5,
            0.0,
        ),
    ])];

    let img_lit = render_mode(
        &mut resources,
        ShadingMode::Lit,
        view,
        proj,
        eye,
        &packed_sun,
        sun_shadow,
        None,
        groups.clone(),
    );
    let img_unlit = render_mode(
        &mut resources,
        ShadingMode::Unlit,
        view,
        proj,
        eye,
        &packed_sun,
        sun_shadow,
        None,
        groups.clone(),
    );
    let lit_sun = sample_world(&img_lit, size, vp, Vec3::new(0.0, -0.3, 3.5));
    let lit_shad = sample_world(&img_lit, size, vp, Vec3::new(0.0, -0.3, 1.0));
    assert!(
        lit_sun[0] > lit_shad[0] + 40,
        "Lit mode must show lighting contrast: sun={lit_sun:?} shad={lit_shad:?}"
    );
    let unlit_sun = sample_world(&img_unlit, size, vp, Vec3::new(0.0, -0.3, 3.5));
    let unlit_shad = sample_world(&img_unlit, size, vp, Vec3::new(0.0, -0.3, 1.0));
    assert!(
        unlit_sun[0].abs_diff(unlit_shad[0]) <= 3 && unlit_shad[0] > 180,
        "Unlit mode must bypass lighting & shadows: sun={unlit_sun:?} shad={unlit_shad:?}"
    );

    // Wireframe
    let wire_eye = Vec3::new(0.0, 0.0, 5.0);
    let wire_view = Mat4::look_at_rh(wire_eye, Vec3::ZERO, Vec3::Y);
    let wire_groups = vec![cube_group(vec![instance(
        Vec3::ZERO,
        Vec3::new(2.0, 2.0, 2.0),
        [0.9, 0.9, 0.9],
        0.5,
        0.0,
    )])];
    let img_wire = render_mode(
        &mut resources,
        ShadingMode::Wireframe,
        wire_view,
        proj,
        wire_eye,
        &packed_sun,
        None,
        None,
        wire_groups,
    );
    let dark_bg = img_wire
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] < 15 && px[1] < 15 && px[2] < 15)
        .count();
    let bright_edges = img_wire
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] > 70 || px[1] > 70 || px[2] > 70)
        .count();
    assert!(
        dark_bg > (size[0] * size[1] / 2) as usize && bright_edges > 50,
        "Wireframe must render edges over empty background: dark={dark_bg} edges={bright_edges}"
    );

    // Normals (+Z face vs +Y floor)
    let img_normals = render_mode(
        &mut resources,
        ShadingMode::Normals,
        view,
        proj,
        eye,
        &packed_sun,
        None,
        None,
        groups.clone(),
    );
    let n_front = sample_world(&img_normals, size, vp, Vec3::new(-2.5, 0.8, 1.7));
    let n_floor = sample_world(&img_normals, size, vp, Vec3::new(0.0, -0.3, 3.5));
    assert!(
        n_front[2] > 235 && n_floor[1] > 235,
        "Normals must encode +Z={n_front:?} and +Y={n_floor:?}"
    );

    // Roughness (glossy 0.15 vs matte 0.85)
    let img_rough = render_mode(
        &mut resources,
        ShadingMode::Roughness,
        view,
        proj,
        eye,
        &packed_sun,
        None,
        None,
        groups.clone(),
    );
    let r_gloss = sample_world(&img_rough, size, vp, Vec3::new(-2.5, 0.8, 1.7));
    let r_matte = sample_world(&img_rough, size, vp, Vec3::new(2.5, 0.8, 1.7));
    assert_eq!(r_gloss[0], r_gloss[1]);
    assert!(
        r_matte[0] > r_gloss[0] + 100,
        "Roughness must distinguish glossy={r_gloss:?} vs matte={r_matte:?}"
    );

    // Metallic (metal 0.95 vs dielectric 0.05)
    let img_metal = render_mode(
        &mut resources,
        ShadingMode::Metallic,
        view,
        proj,
        eye,
        &packed_sun,
        None,
        None,
        groups.clone(),
    );
    let m_metal = sample_world(&img_metal, size, vp, Vec3::new(-2.5, 0.8, 1.7));
    let m_diel = sample_world(&img_metal, size, vp, Vec3::new(2.5, 0.8, 1.7));
    assert_eq!(m_metal[0], m_metal[2]);
    assert!(
        m_metal[0] > 220 && m_diel[0] < 80,
        "Metallic must distinguish metal={m_metal:?} vs dielectric={m_diel:?}"
    );

    // Albedo (red vs green)
    let img_albedo = render_mode(
        &mut resources,
        ShadingMode::Albedo,
        view,
        proj,
        eye,
        &packed_sun,
        None,
        None,
        groups.clone(),
    );
    let a_red = sample_world(&img_albedo, size, vp, Vec3::new(-2.5, 0.8, 1.7));
    let a_green = sample_world(&img_albedo, size, vp, Vec3::new(2.5, 0.8, 1.7));
    assert!(
        a_red[0] > 200 && a_red[1] < 100 && a_green[1] > 200 && a_green[0] < 100,
        "Albedo must isolate base colors: red={a_red:?} green={a_green:?}"
    );

    // Depth (near panel vs far panel)
    let img_depth = render_mode(
        &mut resources,
        ShadingMode::Depth,
        view,
        proj,
        eye,
        &packed_sun,
        None,
        None,
        groups,
    );
    let d_near = sample_world(&img_depth, size, vp, Vec3::new(2.5, 0.8, 1.7));
    let d_far = sample_world(&img_depth, size, vp, Vec3::new(0.0, 3.0, -13.75));
    assert!(
        d_near[0] > d_far[0] + 60,
        "Depth must brighten nearer surfaces: near={d_near:?} far={d_far:?}"
    );

    // --- 2. AO (SSAO corner darkening) ---
    let eye_ao = Vec3::new(0.0, 2.5, 5.5);
    let view_ao = Mat4::look_at_rh(eye_ao, Vec3::new(0.0, 0.5, 0.0), Vec3::Y);
    let vp_ao = proj * view_ao;
    let ao_groups = vec![cube_group(vec![
        instance(
            Vec3::new(0.0, -0.1, 2.0),
            Vec3::new(10.0, 0.2, 10.0),
            [1.0; 3],
            0.9,
            0.0,
        ),
        instance(
            Vec3::new(0.0, 2.0, -0.1),
            Vec3::new(10.0, 4.0, 0.2),
            [1.0; 3],
            0.9,
            0.0,
        ),
    ])];
    let ao_post = Some(PostProcess {
        ssao: SsaoSettings {
            enabled: true,
            radius: 1.2,
            intensity: 2.5,
            bias: 0.01,
        },
        anti_aliasing: AntiAliasing::Msaa4x,
        ..Default::default()
    });
    let img_ao = render_mode(
        &mut resources,
        ShadingMode::Ao,
        view_ao,
        proj,
        eye_ao,
        &packed_sun,
        None,
        ao_post,
        ao_groups,
    );
    let ao_corner = sample_world(&img_ao, size, vp_ao, Vec3::new(0.0, 0.0, 0.15));
    let ao_open = sample_world(&img_ao, size, vp_ao, Vec3::new(0.0, 0.0, 2.2));
    assert!(
        ao_open[0] > ao_corner[0] + 25 && ao_corner[0] == ao_corner[1],
        "AO mode must show SSAO corner darkening: corner={ao_corner:?} open={ao_open:?}"
    );

    // --- 3. Clusters (left tile 0 lights vs right tile dense lights) ---
    let mut cl_lights = BTreeMap::new();
    for i in 0..8 {
        let mut pt = Light::point(None);
        pt.intensity = 2.0;
        pt.kind = LightKind::Point {
            range: 4.0,
            shadow: Default::default(),
        };
        pt.transform.translation = [3.0, 0.5, 0.2 * i as f32];
        cl_lights.insert(format!("pt-{i}"), pt);
    }
    let packed_clusters = PackedLighting::try_pack(&Lighting::Authored {
        ambient: Ambient {
            color: [0.1; 3],
            intensity: 0.1,
        },
        lights: cl_lights,
    })
    .unwrap();
    let cl_eye = Vec3::new(0.0, 0.5, 8.0);
    let cl_view = Mat4::look_at_rh(cl_eye, Vec3::new(0.0, 0.5, 0.0), Vec3::Y);
    let vp_cl = proj * cl_view;
    let cl_groups = vec![cube_group(vec![instance(
        Vec3::new(0.0, 0.5, 0.0),
        Vec3::new(12.0, 6.0, 0.5),
        [1.0; 3],
        0.5,
        0.0,
    )])];
    let img_clusters = render_mode(
        &mut resources,
        ShadingMode::Clusters,
        cl_view,
        proj,
        cl_eye,
        &packed_clusters,
        None,
        None,
        cl_groups,
    );
    let cl_empty = sample_world(&img_clusters, size, vp_cl, Vec3::new(-3.2, 0.5, 0.25));
    let cl_dense = sample_world(&img_clusters, size, vp_cl, Vec3::new(3.0, 0.5, 0.25));
    assert!(
        cl_empty[2] > cl_empty[0] && cl_dense[1] > cl_empty[1] + 100,
        "Clusters heatmap must contrast empty={cl_empty:?} vs dense={cl_dense:?}"
    );

    // --- 4. Cascades (CSM 4-cascade split palette across depth) ---
    let csm_eye = Vec3::new(0.0, 2.0, 6.0);
    let csm_view = Mat4::look_at_rh(csm_eye, Vec3::new(0.0, 0.0, -25.0), Vec3::Y);
    let csm_proj = Mat4::perspective_rh(55.0_f32.to_radians(), 320.0 / 240.0, 0.5, 80.0);
    let vp_csm = csm_proj * csm_view;
    let csm_groups = vec![cube_group(vec![instance(
        Vec3::new(0.0, -0.5, -35.0),
        Vec3::new(14.0, 0.4, 80.0),
        [0.8; 3],
        0.6,
        0.0,
    )])];
    let img_cascades = render_mode(
        &mut resources,
        ShadingMode::Cascades,
        csm_view,
        csm_proj,
        csm_eye,
        &packed_sun,
        sun_shadow,
        None,
        csm_groups,
    );
    let c0 = sample_world(&img_cascades, size, vp_csm, Vec3::new(0.0, -0.3, 1.5));
    let c1 = sample_world(&img_cascades, size, vp_csm, Vec3::new(0.0, -0.3, -5.0));
    let c2 = sample_world(&img_cascades, size, vp_csm, Vec3::new(0.0, -0.3, -18.0));
    let c3 = sample_world(&img_cascades, size, vp_csm, Vec3::new(0.0, -0.3, -44.0));
    assert!(
        c0[1] > c0[0] && c1[0] > c1[2] && c1[1] > c1[2] && c2[0] > c2[1] && c3[0] > c3[1] + 80,
        "Cascades must color CSM bands distinctly: c0={c0:?} c1={c1:?} c2={c2:?} c3={c3:?}"
    );

    eprintln!(
        "GPU probe: shading_debug_views_gpu_probe passed (all 11 modes verified; Cascades c0={c0:?} c1={c1:?} c2={c2:?} c3={c3:?}; Clusters empty={cl_empty:?} dense={cl_dense:?}; AO corner={ao_corner:?} open={ao_open:?})"
    );
}
