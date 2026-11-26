struct Camera { view_projection: mat4x4<f32>, eye_mode: vec4<f32> };
struct Instance {
    model0: vec4<f32>,
    model1: vec4<f32>,
    model2: vec4<f32>,
    model3: vec4<f32>,
    normal0: vec4<f32>,
    normal1: vec4<f32>,
    normal2: vec4<f32>,
    color: vec4<f32>,
    base_color: vec4<f32>,
    response: vec4<f32>,
    emission_color: vec4<f32>,
    uv_transform: vec4<f32>,
    pbr_params: vec4<f32>,
    pbr_maps: vec4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage, read> instances: array<Instance>;
@group(1) @binding(0) var base_color_texture: texture_2d<f32>;
@group(1) @binding(1) var base_color_sampler: sampler;
@group(1) @binding(2) var roughness_texture: texture_2d<f32>;
@group(1) @binding(3) var metallic_texture: texture_2d<f32>;
@group(1) @binding(4) var normal_texture: texture_2d<f32>;
@group(1) @binding(5) var emissive_texture: texture_2d<f32>;
@group(1) @binding(6) var occlusion_texture: texture_2d<f32>;
struct PointLight {
    position_inverse_range_squared: vec4<f32>,
    color_intensity: vec4<f32>,
};
struct Lighting {
    // x:legacy/authored, y:enabled local count, z:enabled sun, w:has spots.
    mode_counts: vec4<u32>,
    ambient: vec4<f32>,
    direction_to_light: vec4<f32>,
    directional_color_intensity: vec4<f32>,
    points: array<PointLight, 4>,
    spot_cones: array<vec4<f32>, 4>,
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
};
@group(2) @binding(0) var<uniform> lighting: Lighting;
@group(2) @binding(1) var ibl_specular: texture_cube<f32>;
@group(2) @binding(2) var ibl_brdf_lut: texture_2d<f32>;
@group(2) @binding(3) var ibl_sampler: sampler;
@group(2) @binding(4) var<uniform> ibl: IblUniform;
struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) base_color: vec4<f32>,
    @location(4) world_position: vec3<f32>,
    @location(5) @interpolate(flat) response: vec4<f32>,
    @location(6) @interpolate(flat) emission_color: vec4<f32>,
    @location(7) tangent: vec4<f32>,
    @location(8) @interpolate(flat) pbr_params: vec4<f32>,
    @location(9) @interpolate(flat) pbr_maps: vec4<f32>,
};
@vertex fn vs(v: Vertex, @builtin(instance_index) instance_index: u32) -> Output {
    var o: Output;
    let inst = instances[instance_index];
    let model = mat4x4<f32>(inst.model0, inst.model1, inst.model2, inst.model3);
    let normal = mat3x3<f32>(inst.normal0.xyz, inst.normal1.xyz, inst.normal2.xyz);
    o.position = camera.view_projection * model * vec4<f32>(v.position, 1.0);
    o.normal = normal * v.normal;
    o.color = inst.color;
    // OBJ V grows upward; decoded PNG rows grow downward. Convert once here.
    o.uv = vec2<f32>(v.uv.x, 1.0 - v.uv.y) * inst.uv_transform.xy + inst.uv_transform.zw;
    o.base_color = inst.base_color;
    o.response = inst.response;
    o.emission_color = inst.emission_color;
    o.world_position = (model * vec4<f32>(v.position, 1.0)).xyz;
    o.tangent = vec4<f32>(safe_unit((model * vec4<f32>(v.tangent.xyz, 0.0)).xyz), v.tangent.w);
    o.pbr_params = inst.pbr_params;
    o.pbr_maps = inst.pbr_maps;
    return o;
}
// Direct-lit metallic-roughness PBR: GGX NDF, height-correlated Smith visibility,
// Schlick Fresnel F0 = mix(0.04, albedo, metallic), energy-shared Lambert diffuse.
fn safe_unit(value: vec3<f32>) -> vec3<f32> {
    return value * inverseSqrt(max(dot(value, value), 1e-20));
}
fn direct_response(albedo: vec3<f32>, n: vec3<f32>, view: vec3<f32>, light: vec3<f32>, response: vec4<f32>, metallic: f32) -> vec3<f32> {
    let nl = max(dot(n, light), 0.0);
    if response.x < 0.5 { return albedo * nl; }
    let nv = max(dot(n, view), 0.0);
    if nl <= 0.0 || nv <= 0.0 { return vec3<f32>(0.0); }
    let half_vector = safe_unit(view + light);
    let nh = max(dot(n, half_vector), 0.0);
    let vh = clamp(dot(view, half_vector), 0.0, 1.0);
    let roughness = clamp(response.y, 0.08, 1.0);
    let alpha = roughness * roughness;
    let a2 = alpha * alpha;
    // This form avoids cancellation close to NdotH=1 at low roughness.
    let cross_nh = cross(n, half_vector);
    let denominator = dot(cross_nh, cross_nh) + a2 * nh * nh;
    let distribution = a2 / (3.14159265359 * max(denominator * denominator, 1e-20));
    let lambda_v = nl * sqrt(nv * nv * (1.0 - a2) + a2);
    let lambda_l = nv * sqrt(nl * nl * (1.0 - a2) + a2);
    let visibility = 0.5 / max(lambda_v + lambda_l, 1e-20);
    let f0 = mix(vec3<f32>(0.04), albedo, metallic);
    let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - vh, 5.0);
    let diffuse = albedo * (1.0 - metallic) * ((vec3<f32>(1.0) - fresnel) / 3.14159265359);
    return (diffuse + distribution * visibility * fresnel) * nl;
}
// Saturate at the finite RGBA16F storage ceiling. Permitted grazing GGX
// configurations can exceed it even with bounded lights and roughness>=.08.
// This storage safeguard loses radiance above65504; it is not energy preservation.
fn hdr_storage(radiance: vec3<f32>) -> vec4<f32> {
    return vec4<f32>(clamp(radiance, vec3<f32>(0.0), vec3<f32>(65504.0)), 1.0);
}
@fragment fn fs(v: Output) -> @location(0) vec4<f32> {
    return shade(v, v.normal);
}
@fragment fn fs_double_sided(v: Output, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    return shade(v, select(-v.normal, v.normal, front));
}
fn shade(v: Output, normal: vec3<f32>) -> vec4<f32> {
    let sampled = textureSample(base_color_texture, base_color_sampler, v.uv).rgb;
    // Explicit gradients are computed outside per-map branches.
    // No-map draws preserve scalar arithmetic and avoid extra texture fetches.
    let uv_dx = dpdx(v.uv);
    let uv_dy = dpdy(v.uv);
    var response = v.response;
    if response.z > 0.5 {
        response.y = clamp(response.y * textureSampleGrad(roughness_texture, base_color_sampler, v.uv, uv_dx, uv_dy).r, 0.08, 1.0);
    }
    var metallic = clamp(v.pbr_params.x, 0.0, 1.0);
    if v.pbr_maps.x > 0.5 {
        metallic = clamp(metallic * textureSampleGrad(metallic_texture, base_color_sampler, v.uv, uv_dx, uv_dy).r, 0.0, 1.0);
    }
    var n = safe_unit(normal);
    if v.pbr_maps.y > 0.5 {
        let ortho_t = v.tangent.xyz - n * dot(v.tangent.xyz, n);
        if dot(ortho_t, ortho_t) > 1e-12 {
            let t = safe_unit(ortho_t);
            let b = cross(n, t) * v.tangent.w;
            let raw_n = textureSampleGrad(normal_texture, base_color_sampler, v.uv, uv_dx, uv_dy).xyz * 2.0 - vec3<f32>(1.0);
            let scaled_n = safe_unit(vec3<f32>(raw_n.xy * v.pbr_params.y, raw_n.z));
            n = safe_unit(t * scaled_n.x + b * scaled_n.y + n * scaled_n.z);
        }
    }
    var emission = v.emission_color.xyz;
    if v.emission_color.w > 0.5 {
        emission *= textureSampleGrad(emissive_texture, base_color_sampler, v.uv, uv_dx, uv_dy).rgb;
    }
    var ao = 1.0;
    if v.pbr_maps.z > 0.5 {
        ao = mix(1.0, textureSampleGrad(occlusion_texture, base_color_sampler, v.uv, uv_dx, uv_dy).r, clamp(v.pbr_params.z, 0.0, 1.0));
    }
    let albedo = sampled * v.base_color.rgb;
    let highlighted = mix(albedo, vec3<f32>(1.0), select(v.base_color.a, 0.0, v.response.w > 0.5));
    // w=1: eye position, w=0: parallel orthographic direction to camera.
    let view = safe_unit(camera.eye_mode.xyz - v.world_position * camera.eye_mode.w);
    let ambient_scale = select(1.0, 0.96, response.x > 0.5);
    let f0_amb = mix(vec3<f32>(0.04), highlighted, metallic);
    let ambient_albedo = select(highlighted, highlighted * (1.0 - metallic) * ambient_scale + f0_amb * (metallic * 0.25), response.x > 0.5);
    var ibl_total = vec3<f32>(0.0);
    if ibl.params.x > 0.5 && response.x > 0.5 {
        let roughness = clamp(response.y, 0.08, 1.0);
        let nv = max(dot(n, view), 1e-4);
        let reflect_dir = reflect(-view, n);
        let brdf = textureSampleLevel(ibl_brdf_lut, ibl_sampler, vec2<f32>(nv, roughness), 0.0).rg;
        let f_ss = f0_amb * brdf.x + vec3<f32>(brdf.y);
        let e_ss = brdf.x + brdf.y;
        let f_avg = f0_amb + (vec3<f32>(1.0) - f0_amb) * (1.0 / 21.0);
        let f_ms = (f_ss * f_avg / max(vec3<f32>(1.0) - f_avg * (1.0 - e_ss), vec3<f32>(1e-4))) * (1.0 - e_ss);
        let k_s = f_ss + f_ms;
        let k_d = (1.0 - metallic) * max(vec3<f32>(1.0) - k_s, vec3<f32>(0.0)) * highlighted;
        let irr = max(
            ibl.sh[0].xyz
                + ibl.sh[1].xyz * n.y
                + ibl.sh[2].xyz * n.z
                + ibl.sh[3].xyz * n.x
                + ibl.sh[4].xyz * (n.x * n.y)
                + ibl.sh[5].xyz * (n.y * n.z)
                + ibl.sh[6].xyz * (3.0 * n.z * n.z - 1.0)
                + ibl.sh[7].xyz * (n.x * n.z)
                + ibl.sh[8].xyz * (n.x * n.x - n.y * n.y),
            vec3<f32>(0.0),
        );
        let spec_occ = clamp(pow(nv + ao, exp2(-16.0 * roughness - 1.0)) - 1.0 + ao, 0.0, 1.0);
        let prefiltered = textureSampleLevel(ibl_specular, ibl_sampler, reflect_dir, roughness * 4.0).rgb;
        ibl_total = k_d * irr * ao + prefiltered * k_s * spec_occ;
    }
    if lighting.mode_counts.x == 0u {
        let incoming = normalize(vec3<f32>(0.4, 0.8, 0.6));
        let fill = select(ambient_albedo * (0.28 * ao), ibl_total, ibl.params.x > 0.5 && response.x > 0.5);
        let stored = hdr_storage(fill + 0.72 * direct_response(highlighted, n, view, incoming, response, metallic) + emission);
        return vec4<f32>(stored.rgb, 1.0);
    }
    var result = ambient_albedo * lighting.ambient.rgb * ao + ibl_total;
    if lighting.mode_counts.z != 0u {
        var direct = lighting.directional_color_intensity.rgb * direct_response(highlighted, n, view, lighting.direction_to_light.xyz, response, metallic);
        // Shadow attenuates the entire Sun contribution, including specular.
        if sun_shadow.parameters.x > 0.5 { direct *= sun_visibility(v.world_position, n); }
        result += direct;
    }
    for (var i = 0u; i < min(lighting.mode_counts.y, 4u); i++) {
        let point = lighting.points[i];
        let delta = point.position_inverse_range_squared.xyz - v.world_position;
        let d2 = dot(delta, delta);
        let direction = delta / sqrt(max(d2, 1e-8));
        let q = min(d2 * point.position_inverse_range_squared.w, 1.0);
        let smooth_range = 1.0 - q * q;
        var attenuation = smooth_range * smooth_range / (1.0 + d2);
        if lighting.mode_counts.w != 0u && point.color_intensity.w > 0.0 {
            let cone = lighting.spot_cones[i];
            // Squared chord equals 2(1 - cos(theta)) for unit vectors.
            // Normalize independently of the distance attenuation's near floor.
            let chord = delta / sqrt(max(d2, 1e-30)) - cone.xyz;
            let angular = clamp((cone.w - dot(chord, chord)) * point.color_intensity.w, 0.0, 1.0);
            attenuation *= angular * angular;
        }
        result += point.color_intensity.rgb * direct_response(highlighted, n, view, direction, response, metallic) * attenuation;
    }
    return vec4<f32>(hdr_storage(result + emission).rgb, 1.0);
}
