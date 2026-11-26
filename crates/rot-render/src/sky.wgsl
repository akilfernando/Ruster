struct SkyCamera {
    inv_view_projection: mat4x4<f32>,
    eye_mode: vec4<f32>,
};

struct IblUniform {
    // x: enabled (>0.5), y: show_backdrop (>0.5), z: intensity, w: sun_enabled (>0.5)
    params: vec4<f32>,
    zenith: vec4<f32>,
    horizon: vec4<f32>,
    ground: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    sh: array<vec4<f32>, 9>,
    fog_color_mode: vec4<f32>,
    fog_params: vec4<f32>,
    fog_extra: vec4<f32>,
};

@group(0) @binding(0) var<uniform> sky_camera: SkyCamera;
@group(1) @binding(4) var<uniform> ibl: IblUniform;

struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex fn vs(@builtin(vertex_index) vertex_index: u32) -> Output {
    let x = f32((vertex_index & 1u) << 2u) - 1.0;
    let y = 1.0 - f32((vertex_index & 2u) << 1u);
    var out: Output;
    out.position = vec4<f32>(x, y, 1.0, 1.0);
    out.ndc = vec2<f32>(x, y);
    return out;
}

fn safe_unit(v: vec3<f32>) -> vec3<f32> {
    return v * inverseSqrt(max(dot(v, v), 1e-20));
}

fn sky_radiance(dir: vec3<f32>) -> vec3<f32> {
    let d = safe_unit(dir);
    let intensity = max(ibl.params.z, 0.0);
    var color: vec3<f32>;
    if d.y >= 0.0 {
        color = mix(ibl.horizon.rgb, ibl.zenith.rgb, pow(clamp(d.y, 0.0, 1.0), 0.55));
    } else {
        color = mix(ibl.horizon.rgb, ibl.ground.rgb, pow(clamp(-d.y, 0.0, 1.0), 0.45));
    }
    var radiance = color * intensity;
    if ibl.params.w > 0.5 {
        let mu = max(dot(d, safe_unit(ibl.sun_dir.xyz)), 0.0);
        let glow = pow(mu, 16.0) * 0.35
            + pow(mu, 128.0) * 1.2
            + smoothstep(0.9992, 0.9997, mu) * 8.0;
        radiance += ibl.sun_color.rgb * glow * intensity;
    }
    if ibl.fog_color_mode.w > 0.5 && ibl.fog_extra.y > 0.0 {
        let horizon_mask = 1.0 - smoothstep(0.0, 0.35, max(d.y, 0.0));
        let sky_fog = clamp(horizon_mask * ibl.fog_extra.y * ibl.fog_extra.x, 0.0, 1.0);
        radiance = mix(radiance, ibl.fog_color_mode.rgb, sky_fog);
    }
    return clamp(radiance, vec3<f32>(0.0), vec3<f32>(65504.0));
}

@fragment fn fs(v: Output) -> @location(0) vec4<f32> {
    var ray_dir: vec3<f32>;
    if sky_camera.eye_mode.w > 0.5 {
        let world_far = sky_camera.inv_view_projection * vec4<f32>(v.ndc, 1.0, 1.0);
        let far_pos = world_far.xyz / select(world_far.w, 1.0, abs(world_far.w) < 1e-6);
        ray_dir = safe_unit(far_pos - sky_camera.eye_mode.xyz);
    } else {
        ray_dir = safe_unit(-sky_camera.eye_mode.xyz);
    }
    return vec4<f32>(sky_radiance(ray_dir), 1.0);
}
