struct Camera { view_projection: mat4x4<f32> };
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
};
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage, read> instances: array<Instance>;
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
    @location(2) uv: vec2<f32>,
};
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) base_color: vec4<f32>,
    @location(4) world_position: vec3<f32>,
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
    o.uv = vec2<f32>(v.uv.x, 1.0 - v.uv.y);
    o.base_color = inst.base_color;
    o.world_position = (model * vec4<f32>(v.position, 1.0)).xyz;
    return o;
}
@fragment fn fs(v: Output) -> @location(0) vec4<f32> {
    let diffuse = max(dot(normalize(v.normal), normalize(vec3<f32>(0.4, 0.8, 0.6))), 0.0);
    let sampled = textureSample(base_color_texture, base_color_sampler, v.uv).rgb;
    let albedo = sampled * v.base_color.rgb;
    let highlighted = mix(albedo, vec3<f32>(1.0), v.base_color.a);
    if lighting.mode_counts.x == 0u {
        // Preserve V1 arithmetic; zero authored lights never select this path.
        return vec4<f32>(highlighted * (0.28 + 0.72 * diffuse), 1.0);
    }
    let n = v.normal / sqrt(max(dot(v.normal, v.normal), 1e-8));
    var light_sum = lighting.ambient.rgb;
    if lighting.mode_counts.z != 0u {
        light_sum += lighting.directional_color_intensity.rgb * max(dot(n, lighting.direction_to_light.xyz), 0.0);
    }
    for (var i = 0u; i < min(lighting.mode_counts.y, 4u); i++) {
        let point = lighting.points[i];
        let delta = point.position_inverse_range_squared.xyz - v.world_position;
        let d2 = dot(delta, delta);
        let direction = delta / sqrt(max(d2, 1e-8));
        let q = min(d2 * point.position_inverse_range_squared.w, 1.0);
        let smooth_range = 1.0 - q * q;
        let attenuation = smooth_range * smooth_range / (1.0 + d2);
        light_sum += point.color_intensity.rgb * (max(dot(n, direction), 0.0) * attenuation);
    }
    // Relative diffuse output in the existing SDR target; no HDR/exposure claim.
    return vec4<f32>(clamp(highlighted * light_sum, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
