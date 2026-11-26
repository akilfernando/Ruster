struct SsaoUniform {
    projection: mat4x4<f32>,
    inv_projection: mat4x4<f32>,
    params: vec4<f32>, // x: radius, y: intensity, z: bias, w: ortho_flag
    sizes: vec4<f32>,  // xy: 1/half_w, 1/half_h, zw: full_w, full_h
};

@group(0) @binding(0) var<uniform> u: SsaoUniform;
@group(0) @binding(1) var depth_tex: texture_depth_2d;
@group(0) @binding(2) var ao_raw_tex: texture_2d<f32>;

struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex fn vs(@builtin(vertex_index) index: u32) -> Output {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var o: Output;
    o.position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return o;
}

fn load_depth(coord: vec2<i32>, dims: vec2<i32>) -> f32 {
    let clamped = clamp(coord, vec2<i32>(0), dims - vec2<i32>(1));
    return textureLoad(depth_tex, clamped, 0);
}

fn view_pos_from_depth(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let clip = vec4<f32>(ndc, depth, 1.0);
    let vp = u.inv_projection * clip;
    return vp.xyz / select(vp.w, 1.0, abs(vp.w) < 1e-6);
}

fn interleaved_gradient_noise(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

@fragment fn fs_ssao(v: Output) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(max(i32(u.sizes.z), 1), max(i32(u.sizes.w), 1));
    let inv_full = vec2<f32>(1.0) / vec2<f32>(dims);
    let coord = clamp(vec2<i32>(v.uv * vec2<f32>(dims)), vec2<i32>(0), dims - vec2<i32>(1));
    let center_uv = (vec2<f32>(coord) + vec2<f32>(0.5)) * inv_full;
    let d_center = load_depth(coord, dims);
    if d_center >= 0.9999 {
        return vec4<f32>(1.0, 1000.0, 0.0, 1.0);
    }
    let center = view_pos_from_depth(center_uv, d_center);

    let d_l = load_depth(coord + vec2<i32>(-1, 0), dims);
    let d_r = load_depth(coord + vec2<i32>(1, 0), dims);
    let d_u = load_depth(coord + vec2<i32>(0, -1), dims);
    let d_d = load_depth(coord + vec2<i32>(0, 1), dims);

    let dx = select(
        center - view_pos_from_depth(center_uv - vec2<f32>(inv_full.x, 0.0), d_l),
        view_pos_from_depth(center_uv + vec2<f32>(inv_full.x, 0.0), d_r) - center,
        abs(d_r - d_center) < abs(d_center - d_l),
    );
    let dy = select(
        center - view_pos_from_depth(center_uv - vec2<f32>(0.0, inv_full.y), d_u),
        view_pos_from_depth(center_uv + vec2<f32>(0.0, inv_full.y), d_d) - center,
        abs(d_d - d_center) < abs(d_center - d_u),
    );
    var normal = cross(dy, dx);
    let n_len = length(normal);
    if n_len < 1e-6 {
        return vec4<f32>(1.0, -center.z, 0.0, 1.0);
    }
    normal /= n_len;
    if normal.z < 0.0 {
        normal = -normal;
    }

    let up_ref = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), abs(normal.y) < 0.99);
    let tangent = normalize(cross(up_ref, normal));
    let bitangent = cross(normal, tangent);

    let rot = interleaved_gradient_noise(v.position.xy) * 6.283185307;
    let radius = u.params.x;
    let intensity = u.params.y;
    let bias = u.params.z;

    var occlusion = 0.0;
    let sample_count = 12u;
    for (var i = 0u; i < sample_count; i = i + 1u) {
        let fi = (f32(i) + 0.5) / f32(sample_count);
        let angle = fi * 23.999632 + rot;
        let r = sqrt(fi);
        let z_dir = mix(0.2, 1.0, fract(fi * 3.71));
        let xy_scale = sqrt(max(1.0 - z_dir * z_dir, 0.0));
        let dir = tangent * (cos(angle) * xy_scale) + bitangent * (sin(angle) * xy_scale) + normal * z_dir;
        let sample_view = center + dir * (radius * r);

        let proj = u.projection * vec4<f32>(sample_view, 1.0);
        let ndc = proj.xyz / select(proj.w, 1.0, abs(proj.w) < 1e-6);
        let s_uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
        if s_uv.x < 0.0 || s_uv.x > 1.0 || s_uv.y < 0.0 || s_uv.y > 1.0 {
            continue;
        }
        let s_coord = clamp(vec2<i32>(s_uv * vec2<f32>(dims)), vec2<i32>(0), dims - vec2<i32>(1));
        let s_texel_uv = (vec2<f32>(s_coord) + vec2<f32>(0.5)) * inv_full;
        let d_sample = load_depth(s_coord, dims);
        if d_sample >= 0.9999 {
            continue;
        }
        let geom_view = view_pos_from_depth(s_texel_uv, d_sample);
        let delta = geom_view - center;
        let dist = length(delta);
        if dist < 1e-4 {
            continue;
        }
        let eff_bias = bias + max(-center.z, 0.0) * 0.0025;
        let range_check = smoothstep(0.0, 1.0, radius / max(dist, 1e-4));
        if dot(delta, normal) > eff_bias && geom_view.z >= sample_view.z + eff_bias {
            occlusion += range_check;
        }
    }

    let raw_ao = clamp(1.0 - occlusion / f32(sample_count), 0.0, 1.0);
    let ao = pow(raw_ao, max(intensity, 0.0));
    return vec4<f32>(ao, max(-center.z, 0.0), 0.0, 1.0);
}

@fragment fn fs_blur(v: Output) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(ao_raw_tex, 0));
    let coord = clamp(vec2<i32>(v.position.xy), vec2<i32>(0), dims - vec2<i32>(1));
    let center = textureLoad(ao_raw_tex, coord, 0);
    let c_depth = center.g;
    var sum = 0.0;
    var weight_sum = 0.0;
    for (var y = -2; y <= 2; y = y + 1) {
        for (var x = -2; x <= 2; x = x + 1) {
            let p = clamp(coord + vec2<i32>(x, y), vec2<i32>(0), dims - vec2<i32>(1));
            let s = textureLoad(ao_raw_tex, p, 0);
            let dz = abs(s.g - c_depth);
            let w_depth = exp(-dz * 1.5);
            let w_spatial = exp(-f32(x * x + y * y) * 0.2);
            let w = w_depth * w_spatial;
            sum += s.r * w;
            weight_sum += w;
        }
    }
    let out_ao = select(center.r, sum / weight_sum, weight_sum > 1e-5);
    return vec4<f32>(out_ao, 0.0, 0.0, 1.0);
}
