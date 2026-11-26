struct BakeParams {
    zenith: vec4<f32>,
    horizon: vec4<f32>,
    ground: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    // x: face index (0..5), y: roughness, z: sky intensity, w: sun enabled (>0.5)
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> bake: BakeParams;

struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

const PI: f32 = 3.14159265359;

@vertex fn vs(@builtin(vertex_index) vertex_index: u32) -> Output {
    let x = f32((vertex_index & 1u) << 2u);
    let y = f32((vertex_index & 2u) << 1u);
    var out: Output;
    out.position = vec4<f32>(x - 1.0, 1.0 - y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5, y * 0.5);
    return out;
}

fn safe_unit(v: vec3<f32>) -> vec3<f32> {
    return v * inverseSqrt(max(dot(v, v), 1e-20));
}

fn cube_dir(face: u32, uv: vec2<f32>) -> vec3<f32> {
    let sx = 2.0 * uv.x - 1.0;
    let sy = 1.0 - 2.0 * uv.y;
    switch face {
        case 0u: { return normalize(vec3<f32>(1.0, sy, -sx)); }
        case 1u: { return normalize(vec3<f32>(-1.0, sy, sx)); }
        case 2u: { return normalize(vec3<f32>(sx, 1.0, -sy)); }
        case 3u: { return normalize(vec3<f32>(sx, -1.0, sy)); }
        case 4u: { return normalize(vec3<f32>(sx, sy, 1.0)); }
        default: { return normalize(vec3<f32>(-sx, sy, -1.0)); }
    }
}

fn sky_radiance(dir: vec3<f32>, include_disc: bool) -> vec3<f32> {
    let d = safe_unit(dir);
    let intensity = max(bake.params.z, 0.0);
    var color: vec3<f32>;
    if d.y >= 0.0 {
        color = mix(bake.horizon.rgb, bake.zenith.rgb, pow(clamp(d.y, 0.0, 1.0), 0.55));
    } else {
        color = mix(bake.horizon.rgb, bake.ground.rgb, pow(clamp(-d.y, 0.0, 1.0), 0.45));
    }
    var radiance = color * intensity;
    if bake.params.w > 0.5 {
        let mu = max(dot(d, safe_unit(bake.sun_dir.xyz)), 0.0);
        let halo = pow(mu, 16.0) * 0.35 + pow(mu, 128.0) * 1.2;
        var glow = halo;
        if include_disc {
            glow += smoothstep(0.9992, 0.9997, mu) * 8.0;
        }
        radiance += bake.sun_color.rgb * glow * intensity;
    }
    return clamp(radiance, vec3<f32>(0.0), vec3<f32>(65504.0));
}

fn radical_inverse_vdc(bits_in: u32) -> f32 {
    var bits = (bits_in << 16u) | (bits_in >> 16u);
    bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
    bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
    bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
    bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
    return f32(bits) * 2.3283064365386963e-10;
}

fn hammersley(i: u32, n: u32) -> vec2<f32> {
    return vec2<f32>(f32(i) / f32(n), radical_inverse_vdc(i));
}

fn importance_sample_ggx(xi: vec2<f32>, n: vec3<f32>, roughness: f32) -> vec3<f32> {
    let a = roughness * roughness;
    let phi = 2.0 * PI * xi.x;
    let cos_theta = sqrt((1.0 - xi.y) / max(1.0 + (a * a - 1.0) * xi.y, 1e-6));
    let sin_theta = sqrt(max(1.0 - cos_theta * cos_theta, 0.0));
    let h = vec3<f32>(cos(phi) * sin_theta, sin(phi) * sin_theta, cos_theta);
    let up = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(n.z) < 0.999);
    let tangent = safe_unit(cross(up, n));
    let bitangent = cross(n, tangent);
    return safe_unit(tangent * h.x + bitangent * h.y + n * h.z);
}

fn geometry_smith_ibl(nv: f32, nl: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let k = (a) * 0.5;
    let gv = nv / max(nv * (1.0 - k) + k, 1e-6);
    let gl = nl / max(nl * (1.0 - k) + k, 1e-6);
    return gv * gl;
}

@fragment fn fs_brdf_lut(v: Output) -> @location(0) vec4<f32> {
    let nv = clamp(v.uv.x, 1e-4, 1.0);
    let roughness = clamp(v.uv.y, 1e-4, 1.0);
    let v_dir = vec3<f32>(sqrt(max(1.0 - nv * nv, 0.0)), 0.0, nv);
    let n = vec3<f32>(0.0, 0.0, 1.0);
    var scale = 0.0;
    var bias = 0.0;
    const SAMPLES: u32 = 128u;
    for (var i = 0u; i < SAMPLES; i++) {
        let xi = hammersley(i, SAMPLES);
        let h = importance_sample_ggx(xi, n, roughness);
        let l = safe_unit(2.0 * dot(v_dir, h) * h - v_dir);
        let nl = max(l.z, 0.0);
        let nh = max(h.z, 0.0);
        let vh = max(dot(v_dir, h), 0.0);
        if nl > 0.0 {
            let g = geometry_smith_ibl(nv, nl, roughness);
            let g_vis = (g * vh) / max(nh * nv, 1e-6);
            let fc = pow(1.0 - vh, 5.0);
            scale += (1.0 - fc) * g_vis;
            bias += fc * g_vis;
        }
    }
    return vec4<f32>(scale / f32(SAMPLES), bias / f32(SAMPLES), 0.0, 1.0);
}

@fragment fn fs_prefilter(v: Output) -> @location(0) vec4<f32> {
    let face = u32(round(bake.params.x));
    let n = cube_dir(face, v.uv);
    let roughness = clamp(bake.params.y, 0.0, 1.0);
    if roughness <= 0.001 {
        return vec4<f32>(sky_radiance(n, true), 1.0);
    }
    var prefiltered = vec3<f32>(0.0);
    var total_weight = 0.0;
    const SAMPLES: u32 = 64u;
    for (var i = 0u; i < SAMPLES; i++) {
        let xi = hammersley(i, SAMPLES);
        let h = importance_sample_ggx(xi, n, roughness);
        let l = safe_unit(2.0 * dot(n, h) * h - n);
        let nl = max(dot(n, l), 0.0);
        if nl > 0.0 {
            prefiltered += sky_radiance(l, true) * nl;
            total_weight += nl;
        }
    }
    return vec4<f32>(prefiltered / max(total_weight, 1e-4), 1.0);
}
