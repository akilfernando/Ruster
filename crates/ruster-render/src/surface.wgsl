struct Camera { view_projection: mat4x4<f32>, eye_mode: vec4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var base_color_texture: texture_2d<f32>;
@group(1) @binding(1) var base_color_sampler: sampler;
struct PointLight {
    position_inverse_range_squared: vec4<f32>,
    color_intensity: vec4<f32>,
};
struct Lighting {
    // x:0 legacy/1 authored, y:enabled points, z:enabled directional, w:0.
    mode_counts: vec4<u32>,
    ambient: vec4<f32>,
    direction_to_light: vec4<f32>,
    directional_color_intensity: vec4<f32>,
    points: array<PointLight, 4>,
};
@group(2) @binding(0) var<uniform> lighting: Lighting;
struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) model0: vec4<f32>,
    @location(3) model1: vec4<f32>,
    @location(4) model2: vec4<f32>,
    @location(5) model3: vec4<f32>,
    @location(6) normal0: vec4<f32>,
    @location(7) normal1: vec4<f32>,
    @location(8) normal2: vec4<f32>,
    @location(9) color: vec4<f32>,
    @location(10) uv: vec2<f32>,
    @location(11) base_color: vec4<f32>,
    @location(12) response: vec4<f32>,
};
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) base_color: vec4<f32>,
    @location(4) world_position: vec3<f32>,
    @location(5) @interpolate(flat) response: vec4<f32>,
};
@vertex fn vs(v: Vertex) -> Output {
    var o: Output;
    let model = mat4x4<f32>(v.model0, v.model1, v.model2, v.model3);
    let normal = mat3x3<f32>(v.normal0.xyz, v.normal1.xyz, v.normal2.xyz);
    o.position = camera.view_projection * model * vec4<f32>(v.position, 1.0);
    o.normal = normal * v.normal;
    o.color = v.color;
    // OBJ V grows upward; decoded PNG rows grow downward. Convert once here.
    o.uv = vec2<f32>(v.uv.x, 1.0 - v.uv.y);
    o.base_color = v.base_color;
    o.response = v.response;
    o.world_position = (model * vec4<f32>(v.position, 1.0)).xyz;
    return o;
}
// Direct-lit opaque dielectric: GGX NDF, height-correlated Smith visibility,
// Schlick Fresnel F0=.04, energy-shared Lambert. Perceptual r -> alpha=r².
// Artistic ambient is diffuse fill; no environment reflection is implied.
fn safe_unit(value: vec3<f32>) -> vec3<f32> {
    return value * inverseSqrt(max(dot(value, value), 1e-20));
}
fn direct_response(albedo: vec3<f32>, n: vec3<f32>, view: vec3<f32>, light: vec3<f32>, response: vec4<f32>) -> vec3<f32> {
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
    let fresnel = 0.04 + 0.96 * pow(1.0 - vh, 5.0);
    let diffuse = albedo * ((1.0 - fresnel) / 3.14159265359);
    return (diffuse + vec3<f32>(distribution * visibility * fresnel)) * nl;
}
// Saturate at the finite RGBA16F storage ceiling. Permitted grazing GGX
// configurations can exceed it even with bounded lights and roughness>=.08.
// This storage safeguard loses radiance above65504; it is not energy preservation.
fn hdr_storage(radiance: vec3<f32>) -> vec4<f32> {
    return vec4<f32>(clamp(radiance, vec3<f32>(0.0), vec3<f32>(65504.0)), 1.0);
}
@fragment fn fs(v: Output) -> @location(0) vec4<f32> {
    let sampled = textureSample(base_color_texture, base_color_sampler, v.uv).rgb;
    let albedo = sampled * v.base_color.rgb;
    let highlighted = mix(albedo, vec3<f32>(1.0), v.base_color.a);
    let n = safe_unit(v.normal);
    // w=1: eye position, w=0: parallel orthographic direction to camera.
    let view = safe_unit(camera.eye_mode.xyz - v.world_position * camera.eye_mode.w);
    let ambient_scale = select(1.0, 0.96, v.response.x > 0.5);
    if lighting.mode_counts.x == 0u {
        let incoming = normalize(vec3<f32>(0.4, 0.8, 0.6));
        return hdr_storage(highlighted * (0.28 * ambient_scale) + 0.72 * direct_response(highlighted, n, view, incoming, v.response));
    }
    var result = highlighted * lighting.ambient.rgb * ambient_scale;
    if lighting.mode_counts.z != 0u {
        var direct = lighting.directional_color_intensity.rgb * direct_response(highlighted, n, view, lighting.direction_to_light.xyz, v.response);
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
        let attenuation = smooth_range * smooth_range / (1.0 + d2);
        result += point.color_intensity.rgb * direct_response(highlighted, n, view, direction, v.response) * attenuation;
    }
    return hdr_storage(result);
}
